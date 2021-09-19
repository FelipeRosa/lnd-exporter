use std::{net::SocketAddr, sync::Arc};

use clap::Clap;
use prometheus::Encoder;
use tokio::{io::AsyncReadExt, sync::Mutex};

#[derive(Clap)]
#[clap(version = "0.1.0", author = "Felipe Rosa <felipe.sgrosa@gmail.com>")]
struct Opts {
    #[clap(long)]
    macaroon_path: String,
    #[clap(long)]
    tls_cert_path: String,
    #[clap(long, default_value = "https://127.0.0.1:10009")]
    lnd_endpoint: String,
    #[clap(long, default_value = "127.0.0.1:29090")]
    exporter_listen_addr: SocketAddr,
}

lazy_static::lazy_static! {
    static ref LND_NUM_PEERS: prometheus::IntGauge = prometheus::register_int_gauge!(
        "lnd_num_peers",
        "Number of peers connected to the lnd node"
    ).unwrap();

    static ref LND_BLOCK_HEIGHT: prometheus::IntGauge = prometheus::register_int_gauge!(
        "lnd_block_height",
        "Chain block height"
    ).unwrap();

    static ref LND_NUM_ACTIVE_CHANNELS: prometheus::IntGauge = prometheus::register_int_gauge!(
        "lnd_active_channels_count",
        "Number of active channels on the lnd node"
    ).unwrap();

    static ref LND_NUM_INACTIVE_CHANNELS: prometheus::IntGauge = prometheus::register_int_gauge!(
        "lnd_inactive_channels_count",
        "Number of inactive channels on the lnd node"
    ).unwrap();

    static ref LND_NUM_PENDING_CHANNELS: prometheus::IntGauge = prometheus::register_int_gauge!(
        "lnd_pending_channels_count",
        "Number of pending channels on the lnd node"
    ).unwrap();

    static ref LND_TOTAL_CHANNEL_BALANCE_MSAT: prometheus::IntGaugeVec = prometheus::register_int_gauge_vec!(
        "lnd_total_channel_balance_msat",
        "Categorized total funds across all open channels in millisats",
        &["category"]
    ).unwrap();

    static ref LND_OUTGOING_PAYMENTS: prometheus::IntGaugeVec = prometheus::register_int_gauge_vec!(
        "lnd_outgoing_payments",
        "Number of outgoing payments on the lnd node",
        &["status"]
    ).unwrap();

    static ref LND_PAYMENT_FAILURE_REASON: prometheus::IntGaugeVec = prometheus::register_int_gauge_vec!(
        "lnd_payment_failure_reason",
        "Payment failure reason",
        &["reason"]
    ).unwrap();

    static ref LND_CHANNEL_BALANCE: prometheus::IntGaugeVec = prometheus::register_int_gauge_vec!(
        "lnd_channel_balance_sat",
        "Individual channel balances",
        &["chan_id", "active", "channel_point", "category"]
    ).unwrap();

    static ref LND_CHANNEL_TOTAL_SATOSHI_SENT: prometheus::IntGaugeVec = prometheus::register_int_gauge_vec!(
        "lnd_channel_total_satoshi_sent",
        "Amount sent on a given channel",
        &["chan_id", "active", "channel_point"]
    ).unwrap();

    static ref LND_CHANNEL_TOTAL_SATOSHI_RECEIVED: prometheus::IntGaugeVec = prometheus::register_int_gauge_vec!(
        "lnd_channel_total_satoshi_received",
        "Amount received on a given channel",
        &["chan_id", "active", "channel_point"]
    ).unwrap();
}

async fn handler(
    req: hyper::Request<hyper::Body>,
    client: Arc<Mutex<lnrpc::LndClient>>,
) -> Result<hyper::Response<hyper::Body>, hyper::http::Error> {
    match (req.method(), req.uri().path()) {
        (&hyper::http::Method::GET, "/health") => Ok(hyper::http::response::Builder::new()
            .status(200)
            .body(hyper::Body::empty())?),

        (&hyper::http::Method::GET, "/metrics") => {
            // Prevent concurrent collection of metrics
            let mut client = client.lock().await;

            {
                let res = client
                    .get_info(lnrpc::GetInfoRequest {})
                    .await
                    .expect("get info ok");

                LND_NUM_PEERS.set(res.get_ref().num_peers.into());
                LND_NUM_ACTIVE_CHANNELS.set(res.get_ref().num_active_channels.into());
                LND_NUM_INACTIVE_CHANNELS.set(res.get_ref().num_inactive_channels.into());
                LND_NUM_PENDING_CHANNELS.set(res.get_ref().num_pending_channels.into());
            }

            {
                let res = client
                    .list_payments(lnrpc::ListPaymentsRequest {
                        include_incomplete: true,
                        index_offset: 0,
                        max_payments: 0,
                        reversed: false,
                    })
                    .await
                    .expect("list payments ok");

                let mut unknown_payments_count: i64 = 0;
                let mut in_flight_payments_count: i64 = 0;
                let mut succeeded_payments_count: i64 = 0;
                let mut failed_payments_count: i64 = 0;

                let mut failure_reason_none_count: i64 = 0;
                let mut failure_reason_timeout_count: i64 = 0;
                let mut failure_reason_no_route_count: i64 = 0;
                let mut failure_reason_error_count: i64 = 0;
                let mut failure_reason_incorrect_payment_details_count: i64 = 0;
                let mut failure_reason_insufficient_balance_count: i64 = 0;

                for payment in res.get_ref().payments.iter() {
                    match payment.status() {
                        lnrpc::payment::PaymentStatus::Unknown => unknown_payments_count += 1,
                        lnrpc::payment::PaymentStatus::InFlight => in_flight_payments_count += 1,
                        lnrpc::payment::PaymentStatus::Succeeded => succeeded_payments_count += 1,
                        lnrpc::payment::PaymentStatus::Failed => failed_payments_count += 1,
                    }

                    match payment.failure_reason() {
                        lnrpc::PaymentFailureReason::FailureReasonNone => {
                            failure_reason_none_count += 1
                        }
                        lnrpc::PaymentFailureReason::FailureReasonTimeout => {
                            failure_reason_timeout_count += 1
                        }
                        lnrpc::PaymentFailureReason::FailureReasonNoRoute => {
                            failure_reason_no_route_count += 1
                        }
                        lnrpc::PaymentFailureReason::FailureReasonError => {
                            failure_reason_error_count += 1
                        }
                        lnrpc::PaymentFailureReason::FailureReasonIncorrectPaymentDetails => {
                            failure_reason_incorrect_payment_details_count += 1
                        }
                        lnrpc::PaymentFailureReason::FailureReasonInsufficientBalance => {
                            failure_reason_insufficient_balance_count += 1
                        }
                    }
                }

                LND_OUTGOING_PAYMENTS
                    .get_metric_with_label_values(&["unknown"])
                    .expect("unknown outgoing payments metric")
                    .set(unknown_payments_count.into());
                LND_OUTGOING_PAYMENTS
                    .get_metric_with_label_values(&["in_flight"])
                    .expect("in_flight outgoing payments metric")
                    .set(in_flight_payments_count.into());
                LND_OUTGOING_PAYMENTS
                    .get_metric_with_label_values(&["succeeded"])
                    .expect("succeeded outgoing payments metric")
                    .set(succeeded_payments_count.into());
                LND_OUTGOING_PAYMENTS
                    .get_metric_with_label_values(&["failed"])
                    .expect("failed outgoing payments metric")
                    .set(failed_payments_count.into());

                LND_PAYMENT_FAILURE_REASON
                    .get_metric_with_label_values(&["none"])
                    .expect("payment failure reason none metric")
                    .set(failure_reason_none_count.into());
                LND_PAYMENT_FAILURE_REASON
                    .get_metric_with_label_values(&["timeout"])
                    .expect("payment failure reason timeout metric")
                    .set(failure_reason_timeout_count.into());
                LND_PAYMENT_FAILURE_REASON
                    .get_metric_with_label_values(&["no_route"])
                    .expect("payment failure reason no_route metric")
                    .set(failure_reason_no_route_count.into());
                LND_PAYMENT_FAILURE_REASON
                    .get_metric_with_label_values(&["error"])
                    .expect("payment failure reason error metric")
                    .set(failure_reason_error_count.into());
                LND_PAYMENT_FAILURE_REASON
                    .get_metric_with_label_values(&["incorrect_payment_details"])
                    .expect("payment failure reason incorrect_payment_details metric")
                    .set(failure_reason_incorrect_payment_details_count.into());
                LND_PAYMENT_FAILURE_REASON
                    .get_metric_with_label_values(&["insufficient_balance"])
                    .expect("payment failure reason insufficient_balance metric")
                    .set(failure_reason_insufficient_balance_count.into());
            }

            {
                let res = client
                    .channel_balance(lnrpc::ChannelBalanceRequest {})
                    .await
                    .expect("channel balance ok");

                if let Some(local_balance) = &res.get_ref().local_balance {
                    LND_TOTAL_CHANNEL_BALANCE_MSAT
                        .get_metric_with_label_values(&["local"])
                        .expect("local channel balance metric")
                        .set((local_balance.msat as i64).into());
                }
                if let Some(remote_balance) = &res.get_ref().remote_balance {
                    LND_TOTAL_CHANNEL_BALANCE_MSAT
                        .get_metric_with_label_values(&["remote"])
                        .expect("remote channel balance metric")
                        .set((remote_balance.msat as i64).into());
                }
                if let Some(pending_open_local_balance) = &res.get_ref().pending_open_local_balance
                {
                    LND_TOTAL_CHANNEL_BALANCE_MSAT
                        .get_metric_with_label_values(&["pending_open_local"])
                        .expect("pending_open_local channel balance metric")
                        .set((pending_open_local_balance.msat as i64).into());
                }
                if let Some(pending_open_remote_balance) =
                    &res.get_ref().pending_open_remote_balance
                {
                    LND_TOTAL_CHANNEL_BALANCE_MSAT
                        .get_metric_with_label_values(&["pending_open_remote"])
                        .expect("pending_open_remote channel balance metric")
                        .set((pending_open_remote_balance.msat as i64).into());
                }
                if let Some(unsettled_local_balance) = &res.get_ref().unsettled_local_balance {
                    LND_TOTAL_CHANNEL_BALANCE_MSAT
                        .get_metric_with_label_values(&["unsettled_local"])
                        .expect("unsettled_local channel balance metric")
                        .set((unsettled_local_balance.msat as i64).into());
                }
                if let Some(unsettled_remote_balance) = &res.get_ref().unsettled_remote_balance {
                    LND_TOTAL_CHANNEL_BALANCE_MSAT
                        .get_metric_with_label_values(&["unsettled_remote"])
                        .expect("unsettled_remote channel balance metric")
                        .set((unsettled_remote_balance.msat as i64).into());
                }
            }

            {
                let res = client
                    .list_channels(lnrpc::ListChannelsRequest {
                        active_only: false,
                        inactive_only: false,
                        public_only: false,
                        private_only: false,
                        peer: vec![],
                    })
                    .await
                    .expect("list channels ok");

                for channel in res.get_ref().channels.iter() {
                    let chan_id = channel.chan_id.to_string();
                    let active = channel.active.to_string();
                    let channel_point = channel.channel_point.to_string();

                    LND_CHANNEL_BALANCE
                        .get_metric_with_label_values(&[&chan_id, &active, &channel_point, "local"])
                        .expect("lnd channel local balance metric")
                        .set(channel.local_balance.into());

                    LND_CHANNEL_BALANCE
                        .get_metric_with_label_values(&[
                            &chan_id,
                            &active,
                            &channel_point,
                            "remote",
                        ])
                        .expect("lnd channel remote balance metric")
                        .set(channel.remote_balance.into());

                    LND_CHANNEL_BALANCE
                        .get_metric_with_label_values(&[
                            &chan_id,
                            &active,
                            &channel_point,
                            "pending",
                        ])
                        .expect("lnd channel remote balance metric")
                        .set(channel.unsettled_balance);

                    LND_CHANNEL_TOTAL_SATOSHI_SENT
                        .get_metric_with_label_values(&[&chan_id, &active, &channel_point])
                        .expect("lnd channel total satoshi sent metric")
                        .set(channel.total_satoshis_sent.into());

                    LND_CHANNEL_TOTAL_SATOSHI_RECEIVED
                        .get_metric_with_label_values(&[&chan_id, &active, &channel_point])
                        .expect("lnd channel total satoshi received metric")
                        .set(channel.total_satoshis_received.into());
                }
            }

            let ms = prometheus::gather();
            let mut buf = vec![];

            match prometheus::TextEncoder::new().encode(&ms, &mut buf) {
                Ok(_) => Ok(hyper::http::response::Builder::default()
                    .status(200)
                    .body(hyper::Body::from(buf))?),
                Err(e) => {
                    eprintln!("Failed to encode metrics: {}", e);

                    Ok(hyper::http::response::Builder::default()
                        .status(500)
                        .body("Failed to encode metrics".into())?)
                }
            }
        }

        _ => Ok(hyper::http::response::Builder::default()
            .status(404)
            .body(hyper::Body::empty())?),
    }
}

#[tokio::main]
async fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let opts = Opts::parse();

    let mut macaroon = vec![];

    tokio::fs::File::open(opts.macaroon_path)
        .await
        .expect("macaroon file opened")
        .read_to_end(&mut macaroon)
        .await
        .expect("read all");
    log::info!("Macaroon loaded");

    let mut tls_cert = vec![];

    tokio::fs::File::open(opts.tls_cert_path)
        .await
        .expect("cert file opened")
        .read_to_end(&mut tls_cert)
        .await
        .expect("read all");
    log::info!("TLS cert loaded");

    let client = Arc::new(Mutex::new(
        lnrpc::new(
            tls_cert,
            macaroon,
            lnrpc::Endpoint::from_shared(opts.lnd_endpoint.clone())
                .expect("valid endpoint address"),
        )
        .await
        .expect("lightning client"),
    ));
    log::info!("Connected to LND node at {}", opts.lnd_endpoint);

    let server = hyper::Server::bind(&opts.exporter_listen_addr).serve(
        hyper::service::make_service_fn(move |sock: &hyper::server::conn::AddrStream| {
            let remote_addr = sock.remote_addr();
            let client = client.clone();

            async move {
                Ok::<_, hyper::http::Error>(hyper::service::service_fn(move |req| {
                    let start_time = std::time::Instant::now();
                    let client = client.clone();

                    async move {
                        let req_path = req.uri().path().to_string();
                        let req_method = req.method().to_string();

                        let res = handler(req, client).await;

                        match &res {
                            Ok(res) => {
                                log::info!(
                                    "{} {} {} {} {}",
                                    req_method,
                                    req_path,
                                    remote_addr,
                                    res.status(),
                                    start_time.elapsed().as_secs_f64(),
                                );
                            }
                            Err(e) => {
                                log::error!("Failed handling request: {:?}", e);
                            }
                        }

                        res
                    }
                }))
            }
        }),
    );
    log::info!("Exporter listening at {:?}", opts.exporter_listen_addr);

    if let Err(e) = server.await {
        eprintln!("Server error: {}", e);
    }
}

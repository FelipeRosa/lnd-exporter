mod collector;

use std::net::SocketAddr;

use clap::Parser;
use prometheus::Encoder;
use tokio::io::AsyncReadExt;

use crate::collector::LndCollector;

#[derive(Parser)]
#[clap(version = "0.1.0", author = "Felipe Rosa <felipe.sgrosa@gmail.com>")]
struct Opts {
    #[clap(long)]
    macaroon_path: Option<String>,
    #[clap(long)]
    tls_cert_path: Option<String>,
    #[clap(long, default_value = "https://localhost:10009")]
    lnd_endpoint: String,
    #[clap(long, default_value = "127.0.0.1:29090")]
    exporter_listen_addr: SocketAddr,
}

async fn handler(
    req: hyper::Request<hyper::Body>,
) -> Result<hyper::Response<hyper::Body>, hyper::http::Error> {
    match (req.method(), req.uri().path()) {
        (&hyper::http::Method::GET, "/health") => Ok(hyper::http::response::Builder::new()
            .status(200)
            .body(hyper::Body::empty())?),

        (&hyper::http::Method::GET, "/metrics") => {
            let ms = tokio::task::spawn_blocking(|| prometheus::gather())
                .await
                .expect("gather");
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

#[tokio::main(flavor = "current_thread")]
async fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let opts = Opts::parse();

    let macaroon = if let Some(macaroon_path) = opts.macaroon_path {
        let mut macaroon_bytes = vec![];

        tokio::fs::File::open(macaroon_path)
            .await
            .expect("macaroon file opened")
            .read_to_end(&mut macaroon_bytes)
            .await
            .expect("read all");

        log::info!("Macaroon loaded");

        Some(macaroon_bytes)
    } else {
        None
    };

    let tls_cert = if let Some(tls_cert_path) = opts.tls_cert_path {
        let mut tls_cert_bytes = vec![];

        tokio::fs::File::open(tls_cert_path)
            .await
            .expect("cert file opened")
            .read_to_end(&mut tls_cert_bytes)
            .await
            .expect("read all");
        log::info!("TLS cert loaded");

        Some(tls_cert_bytes)
    } else {
        None
    };

    let lnd_client = lnrpc::new(
        tls_cert,
        macaroon,
        lnrpc::Endpoint::from_shared(opts.lnd_endpoint.clone()).expect("valid endpoint address"),
    )
    .await
    .expect("lightning client");

    let collector = LndCollector::new(lnd_client);

    prometheus::register(Box::new(collector)).expect("registered collector");

    log::info!("Connected to LND node at {}", opts.lnd_endpoint);

    let server = hyper::Server::bind(&opts.exporter_listen_addr).serve(
        hyper::service::make_service_fn(move |sock: &hyper::server::conn::AddrStream| {
            let remote_addr = sock.remote_addr();

            async move {
                Ok::<_, hyper::http::Error>(hyper::service::service_fn(move |req| {
                    let start_time = std::time::Instant::now();

                    async move {
                        let req_path = req.uri().path().to_string();
                        let req_method = req.method().to_string();

                        let res = handler(req).await;

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

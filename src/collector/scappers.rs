use lnrpc::LndClient;
use prometheus::{core::Collector, proto::MetricFamily};
use tokio::sync::MutexGuard;

use super::ListPaymentsCache;

pub async fn scrape_getinfo(lnd_client: &mut MutexGuard<'_, LndClient>) -> Vec<MetricFamily> {
    let mut metrics = vec![];

    let res = lnd_client.get_info(lnrpc::GetInfoRequest {}).await;

    match res {
        Ok(res) => {
            let num_peers_total = super::metrics::num_peers_total();
            num_peers_total.set(res.get_ref().num_peers.into());
            metrics.extend(num_peers_total.collect());

            let block_height = super::metrics::block_height();
            block_height.set(res.get_ref().block_height.into());
            metrics.extend(block_height.collect());
        }

        Err(e) => {
            log::error!("Failed to collect getinfo metrics ERROR={:?}", e);
        }
    }

    metrics
}

pub async fn scrape_listpayments(
    lnd_client: &mut MutexGuard<'_, LndClient>,
    cache: &mut MutexGuard<'_, ListPaymentsCache>,
) -> Vec<MetricFamily> {
    let mut metrics = vec![];

    let res = lnd_client
        .list_payments(lnrpc::ListPaymentsRequest {
            include_incomplete: true,
            index_offset: cache.index_offset,
            ..lnrpc::ListPaymentsRequest::default()
        })
        .await;

    match res {
        Ok(res) => {
            cache.index_offset = res.get_ref().last_index_offset;

            for payment in res.get_ref().payments.iter() {
                *cache.outgoing_payments.entry(payment.status()).or_default() += 1;

                *cache
                    .payment_failure_reasons
                    .entry(payment.failure_reason())
                    .or_default() += 1;
            }

            let outgoing_payments = super::metrics::outgoing_payments();

            for (status, count) in cache.outgoing_payments.iter() {
                let status_str = match status {
                    lnrpc::payment::PaymentStatus::Unknown => "unknown",
                    lnrpc::payment::PaymentStatus::InFlight => "in_flight",
                    lnrpc::payment::PaymentStatus::Succeeded => "succeeded",
                    lnrpc::payment::PaymentStatus::Failed => "failed",
                };

                outgoing_payments
                    .with_label_values(&[status_str])
                    .set(*count);
            }

            let payment_failure_reasons = super::metrics::payment_failure_reasons();

            for (reason, count) in cache.payment_failure_reasons.iter() {
                let reason_str = match reason {
                    lnrpc::PaymentFailureReason::FailureReasonNone => "none",
                    lnrpc::PaymentFailureReason::FailureReasonTimeout => "timeout",
                    lnrpc::PaymentFailureReason::FailureReasonNoRoute => "no_route",
                    lnrpc::PaymentFailureReason::FailureReasonError => "error",
                    lnrpc::PaymentFailureReason::FailureReasonIncorrectPaymentDetails => {
                        "incorrect_payment_details"
                    }
                    lnrpc::PaymentFailureReason::FailureReasonInsufficientBalance => {
                        "insufficient_balance"
                    }
                };

                payment_failure_reasons
                    .with_label_values(&[reason_str])
                    .set(*count);
            }

            metrics.extend(outgoing_payments.collect());
            metrics.extend(payment_failure_reasons.collect());
        }

        Err(e) => {
            log::error!("Failed to collect listpayments metrics ERROR={:?}", e);
        }
    }

    metrics
}

pub async fn scrape_listchannels(lnd_client: &mut MutexGuard<'_, LndClient>) -> Vec<MetricFamily> {
    let mut metrics = vec![];

    let res = lnd_client
        .list_channels(lnrpc::ListChannelsRequest::default())
        .await;

    match res {
        Ok(res) => {
            let channel_balance_total_sat = super::metrics::channel_balance_total_sat();

            for channel in res.get_ref().channels.iter() {
                let chan_id = channel.chan_id.to_string();
                let active = if channel.active { "true" } else { "false" };
                let channel_point = &channel.channel_point;

                channel_balance_total_sat
                    .with_label_values(&[&chan_id, active, channel_point, "local"])
                    .set(channel.local_balance.into());
                channel_balance_total_sat
                    .with_label_values(&[&chan_id, active, channel_point, "remote"])
                    .set(channel.remote_balance.into());
                channel_balance_total_sat
                    .with_label_values(&[&chan_id, active, channel_point, "unsettled"])
                    .set(channel.unsettled_balance.into());
            }

            metrics.extend(channel_balance_total_sat.collect());
        }

        Err(e) => {
            log::error!("Failed to collect listchannels metrics ERROR={:?}", e);
        }
    }

    metrics
}

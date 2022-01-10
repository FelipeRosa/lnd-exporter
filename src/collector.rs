mod metrics;
mod scappers;

use std::{collections::HashMap, sync::Arc};

use lnrpc::LndClient;
use prometheus::{
    core::{Collector, Desc},
    proto::MetricFamily,
};
use tokio::sync::Mutex;

pub struct ListPaymentsCache {
    index_offset: u64,
    outgoing_payments: HashMap<lnrpc::payment::PaymentStatus, i64>,
    payment_failure_reasons: HashMap<lnrpc::PaymentFailureReason, i64>,
    total_fee_msat: i64,
}

pub struct LndCollector {
    lnd_client: Arc<Mutex<LndClient>>,
    metric_desc: Vec<Desc>,
    listpayments_cache: Arc<Mutex<ListPaymentsCache>>,
}

impl LndCollector {
    pub fn new(lnd_client: LndClient) -> Self {
        Self {
            lnd_client: Arc::new(Mutex::new(lnd_client)),
            metric_desc: vec![
                metrics::num_peers_total().desc(),
                metrics::block_height().desc(),
                metrics::outgoing_payments().desc(),
            ]
            .into_iter()
            .flatten()
            .cloned()
            .collect(),
            listpayments_cache: Arc::new(Mutex::new(ListPaymentsCache {
                index_offset: 0,
                outgoing_payments: HashMap::new(),
                payment_failure_reasons: HashMap::new(),
                total_fee_msat: 0,
            })),
        }
    }
}

impl Collector for LndCollector {
    fn desc(&self) -> Vec<&Desc> {
        self.metric_desc.iter().collect()
    }

    fn collect(&self) -> Vec<MetricFamily> {
        log::info!("Collecting metrics");

        let lnd_client = self.lnd_client.clone();
        let listpayments_cache = self.listpayments_cache.clone();

        log::debug!("Building Tokio runtime");
        let rt = tokio::runtime::Handle::current();

        let metrics = rt.block_on(async {
            // Prevent concurrent collects
            log::debug!("Acquiring collector locks");
            let mut lnd_client_lock = lnd_client.lock().await;
            let mut listpayments_cache_lock = listpayments_cache.lock().await;
            let mut metrics = vec![];

            metrics.extend(scappers::scrape_getinfo(&mut lnd_client_lock).await);
            metrics.extend(
                scappers::scrape_listpayments(&mut lnd_client_lock, &mut listpayments_cache_lock)
                    .await,
            );
            metrics.extend(scappers::scrape_listchannels(&mut lnd_client_lock).await);

            metrics
        });

        log::info!("Done collecting metrics");
        metrics
    }
}

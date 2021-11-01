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
            })),
        }
    }
}

impl Collector for LndCollector {
    fn desc(&self) -> Vec<&Desc> {
        self.metric_desc.iter().collect()
    }

    fn collect(&self) -> Vec<MetricFamily> {
        let build_rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build();

        let rt = match build_rt {
            Ok(rt) => rt,
            Err(e) => {
                log::error!(
                    "Failed to build Collector's collect Tokio runtime ERROR={:?}",
                    e
                );

                return vec![];
            }
        };

        let lnd_client = self.lnd_client.clone();
        let listpayments_cache = self.listpayments_cache.clone();

        std::thread::spawn(move || {
            rt.block_on(async {
                // Prevent concurrent collects
                let mut lnd_client_lock = lnd_client.lock().await;
                let mut listpayments_cache_lock = listpayments_cache.lock().await;
                let mut metrics = vec![];

                metrics.extend(scappers::scrape_getinfo(&mut lnd_client_lock).await);
                metrics.extend(
                    scappers::scrape_listpayments(
                        &mut lnd_client_lock,
                        &mut listpayments_cache_lock,
                    )
                    .await,
                );
                metrics.extend(scappers::scrape_listchannels(&mut lnd_client_lock).await);

                metrics
            })
        })
        .join()
        .unwrap()
    }
}

mod metrics;
mod scappers;

use std::sync::Arc;

use lnrpc::LndClient;
use prometheus::{
    core::{Collector, Desc},
    proto::MetricFamily,
};
use tokio::sync::Mutex;

pub struct LndCollector {
    lnd_client: Arc<Mutex<LndClient>>,
    metric_desc: Vec<Desc>,
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

        // Prevent concurrent collects
        let lnd_client_lock = self.lnd_client.clone();

        std::thread::spawn(move || {
            rt.block_on(async {
                let mut lnd_client = lnd_client_lock.lock().await;
                let mut metrics = vec![];

                metrics.extend(scappers::scrape_getinfo(&mut lnd_client).await);
                metrics.extend(scappers::scrape_listpayments(&mut lnd_client).await);
                metrics.extend(scappers::scrape_listchannels(&mut lnd_client).await);

                metrics
            })
        })
        .join()
        .unwrap()
    }
}

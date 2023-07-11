use actix_web;
use log::debug;
use prometheus_client::encoding::EncodeLabelSet;
use prometheus_client::metrics::family::Family;
use prometheus_client::metrics::gauge::Gauge;
use prometheus_client::registry::Registry;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tokio::time;
use tokio::time::*;

pub mod dell;
pub mod hpe;
pub mod lenovo;
use super::settings::Console;
use super::settings::Settings;

use dell::collect_dell_metrics;
use hpe::collect_hpe_metrics;
use lenovo::collect_lenovo_metrics;

#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet)]
pub struct NodeLabels {
    pub name: String,
}
#[derive(Serialize, Deserialize, Debug, Clone, Eq, PartialEq, Hash, EncodeLabelSet)]
pub struct Node {
    pub device_name: String,
    pub model: String,
    pub status: u16,
    pub connection_state: u8,
    pub power_state: u16,
    pub compliant: u8,
}

#[derive(Debug)]
pub struct Exporter {
    //collectors: Vec<Box<dyn traits::Collector>>,
    settings: Settings,
    nodes: Vec<String>,
    pub registry: Registry,
    pub metrics: Family<Node, Gauge>,
}

impl Exporter {
    pub fn new(s: Settings) -> Exporter {
        Exporter {
            settings: s,
            nodes: vec![],
            registry: Registry::default(),
            metrics: Family::default(),
        }
    }

    pub fn inc_requests(&self, node: Node) {
        self.metrics
            .get_or_create(&node)
            .set(i64::from(node.compliant));
    }

    pub async fn run(&mut self) {
        let (tx, mut rx): (mpsc::Sender<Node>, mpsc::Receiver<Node>) = mpsc::channel(100);
        let tx01 = tx.clone();
        let tx02 = tx.clone();
        let s = self.settings.clone();

        let mut interval = time::interval(Duration::from_secs(self.settings.interval_in_min * 60));
        let m = self.metrics.clone();

        actix_web::rt::spawn(async move {
            loop {
                interval.tick().await;
                debug!("clearing metrics");
                m.clear();
            }
        });

        actix_web::rt::spawn(async move {
            collect_dell_metrics(s.dell, s.interval_in_min, tx).await;
        });
        actix_web::rt::spawn(async move {
            collect_lenovo_metrics(s.lenovo, s.interval_in_min, tx01).await;
        });
        actix_web::rt::spawn(async move {
            collect_hpe_metrics(s.hpe, s.interval_in_min, tx02).await;
        });

        while let Some(n) = rx.recv().await {
            println!("got = {:?}", n.device_name);
            if !self.nodes.contains(&n.device_name) {
                self.nodes.push(n.device_name.clone());
            }
            self.inc_requests(n);
        }
    }
}

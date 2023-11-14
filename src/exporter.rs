use actix_web;
use prometheus_client::encoding::EncodeLabelSet;
use prometheus_client::metrics::family::Family;
use prometheus_client::metrics::gauge::Gauge;
use prometheus_client::registry::Registry;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

pub mod dell;
pub mod hpe;
pub mod lenovo;
pub mod utils;

use super::settings::Console;
use super::settings::Settings;
use super::netbox::Netbox;

use dell::collect_dell_metrics;
use hpe::collect_hpe_metrics;
use lenovo::collect_lenovo_metrics;

#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet)]
pub struct NodeLabels {
    pub name: String,
}
#[derive(Serialize, Deserialize, Debug, Clone, Eq, PartialEq, Hash, EncodeLabelSet, Default)]
pub struct Node {
    pub device_name: String,
    pub model: String,
    pub health_status: u16,
    pub connection_state: u8,
    pub power_state: u16,
    pub compliant: u8,
    pub console: String,
    pub uuid: String,
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

        let netbox = Netbox::new(s.netbox_url.to_owned(), s.query.to_owned());
        let netbox_hpe = netbox.clone();
        let netbox_lenovo = netbox.clone();
        actix_web::rt::spawn(async move {
            collect_dell_metrics(s.dell, netbox.clone(), s.interval_in_min, tx).await;
        });
        actix_web::rt::spawn(async move {
            collect_lenovo_metrics(s.lenovo, netbox_lenovo, s.interval_in_min, tx01).await;
        });
        actix_web::rt::spawn(async move {
            collect_hpe_metrics(s.hpe, netbox_hpe, s.interval_in_min, tx02).await;
        });

        while let Some(n) = rx.recv().await {
            println!("{:?}=>{:?}", n.console, n.device_name);
            if !self.nodes.contains(&n.device_name) {
                self.nodes.push(n.device_name.clone());
            }
            self.inc_requests(n);
        }
    }
}

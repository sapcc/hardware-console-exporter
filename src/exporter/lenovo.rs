use log::{error, info};
use reqwest;
use reqwest::header::{ACCEPT, CONTENT_TYPE};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tokio::time;
use tokio::time::*;

use super::Console;
use super::Node;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Device {
    #[serde(alias = "hostname")]
    pub device_name: String,
    pub model: String,
    #[serde(alias = "Status")]
    pub status: Status,
    #[serde(alias = "powerStatus", default = "default_power_state")]
    pub power_state: u8,
}

fn default_power_state() -> u8 {
    0
}

impl From<Device> for Node {
    fn from(d: Device) -> Self {
        let status = if d.status.name == "MANAGED" { 1 } else { 0 };
        let power = if d.power_state == 5 { 1 } else { 0 };
        let connection = 0;
        Self {
            device_name: d.device_name,
            status,
            model: d.model,
            power_state: power,
            connection_state: connection,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Status {
    message: String,
    name: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct APIResponse {
    #[serde(rename = "nodeList")]
    value: Vec<Device>,
}

pub async fn collect_lenovo_metrics(settings: Console, interval: u64, tx: mpsc::Sender<Node>) {
    let client = match Client::builder().danger_accept_invalid_certs(true).build() {
        Ok(client) => client,
        Err(error) => panic!("error creating reqwest client: {:?}", error),
    };
    info!("lenovo client ready. interval: {}", interval);
    let mut interval = time::interval(Duration::from_secs(interval * 60));
    let mut host = settings.host.clone();
    host.set_path("nodes");
    let url = host.as_str();
    loop {
        let password: Option<String> = settings.password.to_owned();
        interval.tick().await;
        info!("executing lenovo metric collect");

        let resp = match client
            .get(url)
            .header(CONTENT_TYPE, "application/json")
            .header(ACCEPT, "application/json")
            .basic_auth(settings.username.to_string(), password)
            .send()
            .await
        {
            Ok(v) => v,
            Err(e) => {
                error!("error calling lxca: {}", e);
                continue;
            }
        };
        if resp.status() == reqwest::StatusCode::OK {
            let json = match resp.json::<APIResponse>().await {
                Ok(parsed) => parsed,
                Err(e) => {
                    error!("error parsing lxca json: {}", e);
                    continue;
                }
            };

            for device in json.value {
                let node = Node::from(device);
                tx.send(node).await.unwrap_or_else(|error| {
                    error!("error sending node on channel: {:?}", error);
                })
            }
        }
    }
}

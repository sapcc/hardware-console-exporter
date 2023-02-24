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
    #[serde(alias = "DeviceName")]
    pub device_name: String,
    #[serde(alias = "Model", alias = "model")]
    pub model: String,
    #[serde(alias = "Status")]
    pub status: u16,
    #[serde(rename = "ConnectionState")]
    pub connection_state: bool,
    #[serde(alias = "PowerState")]
    pub power_state: u16,
}

impl From<Device> for Node {
    fn from(d: Device) -> Self {
        let status = if d.status == 1000 { 1 } else { 0 };
        let power = if d.power_state == 17 { 1 } else { 0 };
        let connection = if d.connection_state { 1 } else { 0 };
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
struct APIResponse {
    #[serde(alias = "value")]
    value: Vec<Device>,
}

pub async fn collect_dell_metrics(settings: Console, tx: mpsc::Sender<Node>) {
    let client = match Client::builder().danger_accept_invalid_certs(true).build() {
        Ok(client) => client,
        Err(error) => panic!("Problem creating client: {:?}", error),
    };
    info!("dell client ready. interval: {}", settings.interval_in_min);
    let mut interval = time::interval(Duration::from_secs(settings.interval_in_min * 60));
    let mut host = settings.host.clone();
    host.set_path("/api/DeviceService/Devices");
    let url = host.as_str();
    loop {
        let password: Option<String> = settings.password.to_owned();
        interval.tick().await;
        info!("executing dell metric collect");

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
                error!("error calling openmanage: {}", e);
                continue;
            }
        };

        if resp.status() == reqwest::StatusCode::OK {
            let json = match resp.json::<APIResponse>().await {
                Ok(parsed) => parsed,
                Err(e) => {
                    error!("error parsing dell json: {}", e);
                    continue;
                }
            };

            println!("{:?}", json.value[0].device_name);
            for device in json.value {
                let node = Node::from(device);
                tx.send(node).await.unwrap_or_else(|e| {
                    error!("error sending node on channel: {:?}", e);
                })
            }
        }
    }
}

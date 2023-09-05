use log::{error, info};
use reqwest::header::{ACCEPT, CONTENT_TYPE};
use reqwest::Client;
use reqwest::{self};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tokio::time::{Duration, interval};
use url::Url;

use super::Console;
use super::Node;

use crate::exporter::utils::get_request_builder;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Device {
    pub uuid: String,
    #[serde(alias = "hostname")]
    pub device_name: String,
    pub model: String,
    #[serde(alias = "Status")]
    pub status: Status,
    #[serde(alias = "powerStatus", default = "default_power_state")]
    pub power_state: u8,
    #[serde(skip_deserializing)]
    pub compliant: String,
}

fn default_power_state() -> u8 {
    0
}

impl From<Device> for Node {
    fn from(d: Device) -> Self {
        let status = if d.status.name == "MANAGED" { 1 } else { 0 };
        let power = if d.power_state == 5 { 1 } else { 0 };
        let compliant = if d.compliant == "yes" { 1 } else { 0 };
        let connection = 0;
        Self {
            device_name: d.device_name,
            status,
            model: d.model,
            power_state: power,
            connection_state: connection,
            compliant: compliant,
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
#[derive(Serialize, Deserialize, Debug, Clone)]
struct Compliance {
    keep: bool,
    #[serde(rename = "policyName")]
    policy_name: String,
    #[serde(rename = "type")]
    device_type: String,
    uuid: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct CompliancePolicy {
    compliance: Vec<Compliance>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct ComplianceResult {
    #[serde(rename = "policyName")]
    policy_name: String,
    #[serde(rename = "endpointCompliant")]
    endpoint_compliant: String,
}

pub async fn collect_lenovo_metrics(settings: Console, interval_sec: u64, tx: mpsc::Sender<Node>) {
    info!("lenovo client ready. interval: {}", interval_sec);
    let mut interval = interval(Duration::from_secs(interval_sec * 60));
    let mut host = settings.host.clone();
    host.set_path("nodes");

    loop {
        interval.tick().await;
        info!("executing lenovo metric collect");

        let devices = get_nodes(settings.clone(), host.clone()).await.unwrap_or_else(|e| {
            error!("error getting nodes: {}", e);
            vec![]
        });
        for mut device in devices{
            if attach_device_compliance_policy(settings.clone(), device.uuid.to_string())
                .await
                .is_ok()
            {
                set_device_compliance_status(&settings, &mut device).await;
            }
            let node = Node::from(device);
            tx.send(node).await.unwrap_or_else(|error| {
                error!("error sending node on channel: {:?}", error);
            })
        }
    }
}

async fn get_nodes(settings: Console, url: Url) -> Result<(Vec<Device>), reqwest::Error>{
    let resp = get_request_builder(
        reqwest::Method::GET,
        None,
        Some(&settings), 
        url
    )
    .send()
    .await?
    .error_for_status()?
    .json::<APIResponse>()
    .await?;

    Ok(resp.value)
}

async fn attach_device_compliance_policy(
    settings: Console,
    uuid: String,
) -> Result<(), reqwest::Error> {
    let mut host = settings.host.clone();
    host.set_path("/compliancePolicies/compareResult");

    let body = CompliancePolicy {
        compliance: vec![Compliance {
            keep: true,
            policy_name: settings.policy_name.to_string(),
            device_type: "SERVER".to_string(),
            uuid: uuid.to_string(),
        }],
    };

    let _ = get_request_builder(
        reqwest::Method::POST,
        None,
        Some(&settings), 
        host
    )
        .json(&body)
        .send()
        .await?;

    Ok(())
}

async fn set_device_compliance_status(settings: &Console, device: &mut Device) {
    let mut host = settings.host.clone();
    host.set_path("/compliancePolicies/persistedResult");
    host.set_query(Some(format!("type=SERVER&uuid={}", device.uuid).as_str()));
    device.compliant = "no".to_string();
    let resp = match get_request_builder(
        reqwest::Method::GET, 
        None,
        Some(&settings), 
        host,
    )
        .send()
        .await
    {
        Ok(r) => r,
        Err(e) => {
            error!("error calling lxca: {}", e);
            return;
        }
    };

    if resp.status() == reqwest::StatusCode::OK {
        let json = match resp.json::<ComplianceResult>().await {
            Ok(parsed) => parsed,
            Err(e) => {
                error!("error parsing lxca json: {}", e);
                return;
            }
        };
        if json.policy_name == settings.policy_name {
            device.compliant = json.endpoint_compliant.to_string();
        }
    }
}

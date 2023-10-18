use log::{error, info};
use reqwest;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tokio::time::{sleep, Duration, interval};


use crate::exporter::utils::get_request_builder;


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
        let status = if d.status == 1000 { 1 } else { 0 }; //1000: normal, 3000: warning, 4000:critical
        let power = if d.power_state == 17 { 1 } else { 0 };
        let connection = if d.connection_state { 1 } else { 0 };
        Self {
            device_name: d.device_name,
            health_status: status,
            model: d.model,
            power_state: power,
            connection_state: connection,
            compliant: 0,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct APIResponse {
    #[serde(alias = "value")]
    value: Vec<Device>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct Compliance {
    #[serde(rename = "Id")]
    id: u16,
    #[serde(rename = "TaskId")]
    task_id : u16,
    #[serde(rename = "Name")]
    name: String,
    #[serde(rename = "RepositoryName")]
    repository_name: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct CompliancePolicy {
    value: Vec<Compliance>,
}
#[derive(Serialize, Deserialize, Debug, Clone)]
struct ComplianceReport {
    #[serde(rename = "DeviceId")]
    id: u16, 
    #[serde(rename = "DeviceName")] 
    name: String,
    #[serde(rename = "DeviceModel")] 
    model: String,
    #[serde(rename = "FirmwareStatus")]
    firmware_status: String,
    #[serde(rename = "ComplianceStatus")]
    compliance_status: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct ComplianceReports {
    #[serde(alias = "value")]
    value: Vec<ComplianceReport>,
}


pub async fn collect_dell_metrics(settings: Console, interval_sec: u64, tx: mpsc::Sender<Node>) {
    info!("dell client ready. interval: {}", interval_sec);
    let mut interval = interval(Duration::from_secs(interval_sec * 60));
    let comliant_nodes = get_compliant_nodes(settings.clone()).await.unwrap_or_else(|e| {
        error!("error getting compliant nodes: {}", e);
        ComplianceReports{value: vec![]}
    });
    let mut host = settings.host.clone();
    host.set_path("/api/DeviceService/Devices");

    loop {
        interval.tick().await;
        info!("executing dell metric collect");

        let resp = match get_request_builder(
            reqwest::Method::GET, 
            None, 
            Some(&settings), 
            host.clone()
        )
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
            
            for device in json.value {
                let cloned = device.clone();
                let mut node = Node::from(device);
                comliant_nodes.value.iter()
                    .find(|c: &&ComplianceReport| cloned.device_name == c.name)
                    .map(|c| if c.compliance_status == "OK" {node.compliant = 1});
                tx.send(node).await.unwrap_or_else(|e| {
                    error!("error sending node on channel: {:?}", e);
                })
            }
        }
    }
}

async fn get_compliant_nodes(settings: Console) -> Result<ComplianceReports, reqwest::Error>{
    let mut host = settings.host.clone();
    host.set_path("/api/UpdateService/Baselines");

    let json = get_request_builder(
        reqwest::Method::GET, 
        None, 
        Some(&settings), 
        host.clone()
    )
        .send()
        .await?
        .error_for_status()?
        .json::<CompliancePolicy>()
        .await?;

    let task = json.value.iter().find(|v| v.repository_name == "firmware_70u3l" && v.name == "firmware_70u3l");
    match task {
        Some(t) => {
            let mut host = settings.host.clone();
            host.set_path("/api/JobService/Actions/JobService.RunJobs");

            get_request_builder(
                reqwest::Method::POST,
                None,
                Some(&settings), 
                host.clone()
            )
                .json(&serde_json::json!({"JobIds": [t.task_id], "AllJobs":false}))
                .send()
                .await?
                .error_for_status()?;
   
            info!("compliance check started");
            sleep(Duration::from_secs(20)).await;
            info!("compliance check finished");
            let mut host = settings.host.clone();
            host.set_path(format!("/api/UpdateService/Baselines({})/DeviceComplianceReports", t.id).as_str());

            let json = get_request_builder(
                reqwest::Method::GET, 
                None, 
                Some(&settings), 
                host.clone()
            )
                .send()
                .await?
                .error_for_status()?
                .json::<ComplianceReports>()
                .await?;
            return Ok(json);
        }
        None => {
            error!("no compliance task found");
            return Ok(ComplianceReports{value: vec![]})
        }
    }
}
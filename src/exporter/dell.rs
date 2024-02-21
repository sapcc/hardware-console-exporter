use log::{error, info};
use reqwest;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tokio::time::{sleep, Duration, interval};

use crate::exporter::utils::{get_request_builder, deserialize_name};

use super::Console;
use super::Node;
use super::Netbox;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Device {
    #[serde(deserialize_with = "deserialize_name")]
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
    #[serde(alias = "Id")]
    pub id: u16,
}

impl From<Device> for Node {
    fn from(d: Device) -> Self {
        let status = if d.status == 1000 { 1 } else { 0 }; //1000: normal, 3000: warning, 4000:critical
        let power = if d.power_state == 17 { 1 } else { 0 };
        let connection = if d.connection_state { 1 } else { 0 };
        let name = d.device_name.split(".").collect::<Vec<&str>>();
        let name = name[0].to_string().replace("r", "");
        Self {
            device_name: name,
            health_status: status,
            model: d.model,
            power_state: power,
            connection_state: connection,
            compliant: 0,
            console: "na".to_string(),
            uuid: d.id.to_string(),
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
    #[serde(deserialize_with = "deserialize_name")]
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

pub async fn collect_dell_metrics(settings: Console, netbox: Netbox, interval_sec: u64, tx: mpsc::Sender<Node>) {
    info!("dell client ready. interval: {}", interval_sec);
    let mut interval = interval(Duration::from_secs(interval_sec * 60));

    loop {
        interval.tick().await;
        info!("executing dell metric collect");

        let comliant_devices = get_compliant_devices(settings.clone()).await.unwrap_or_else(|e| {
            error!("error getting compliant nodes: {}", e);
            ComplianceReports{value: vec![]}
        });

        let devices = get_devices(settings.clone()).await.unwrap_or_else(|e| {
            error!("error getting compliant nodes: {}", e);
            vec![]
        });

        let mut nodes = devices.clone().into_iter().map(|d| Node::from(d)).collect::<Vec<Node>>();
        comliant_devices.value.iter().for_each(|c|{
            nodes.iter_mut()
                .find(|n| c.name == n.device_name)
                .map(|n| if c.compliance_status == "OK" {n.compliant = 1});
        });

        let netbox_devices = netbox.get_devices_by_manufacturer(settings.manufacturer_name.to_string()).await 
        .unwrap_or_else(|e| {
            error!("error getting netbox devices: {}", e);
            vec![]
        });

        for device in netbox_devices {
            let node = nodes.iter_mut()
                .find(|n| device.name.to_lowercase().contains(n.device_name.to_lowercase().as_str()));
            if node.is_some() {
                let n = node.unwrap();
                n.console = "openmanage".to_string();
                tx.send(n.clone()).await.unwrap();
            } else {
                let n = Node { device_name: device.name, ..Default::default() };
                tx.send(n.clone()).await.unwrap();
            }
        }
    }
}

async fn get_devices(settings: Console) -> Result<Vec<Device>, reqwest::Error>{
    let mut host = settings.host.clone();
    host.set_path("/api/DeviceService/Devices");
    host.set_query(Some(("top=5000")));
    let resp = get_request_builder(
        reqwest::Method::GET,
        None,
        Some(&settings), 
        host
    )
    .send()
    .await?
    .error_for_status()?
    .json::<APIResponse>()
    .await?;

    Ok(resp.value)
}

async fn get_compliant_devices(settings: Console) -> Result<ComplianceReports, reqwest::Error>{
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

    let task = json.value.iter().find(|v| v.repository_name == settings.policy_name && v.name == settings.policy_name);
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
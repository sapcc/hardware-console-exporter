use log::{error, info};
use reqwest;
use tokio::time::{Duration, interval};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

use super::Console;
use super::Node;
use super::Netbox;

use crate::exporter::utils::{get_request_builder, deserialize_name};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Device {
    pub uuid: String,
    #[serde(deserialize_with = "deserialize_name")]
    #[serde(alias = "name")]
    pub device_name: String,
    pub model: String,
    pub status: String,
    #[serde(alias = "powerState")]
    pub power_state: String,
    #[serde(skip_deserializing)]
    pub compliant: String,
}

impl From<Device> for Node {
    fn from(d: Device) -> Self {
        let status = if d.status == "OK" { 1 } else { 0 };
        let power = if d.power_state == "On" { 1 } else { 0 };
        let compliant = if d.compliant == "Compliant" { 1 } else { 0 };
        Self {
            device_name: d.device_name,
            health_status: status,
            model: d.model,
            power_state: power,
            connection_state: 0,
            compliant: compliant,
            console: "na".to_string(),
            uuid: d.uuid,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct APIResponse {
    #[serde(alias = "members")]
    value: Vec<Device>,
}
#[derive(Serialize, Deserialize, Debug, Clone)]
struct Auth {
    #[serde(rename = "authLoginDomain")]
    auth_login_domain: String,
    password: String,
    #[serde(rename = "userName")]
    user_name: String,
}
#[derive(Serialize, Deserialize, Debug, Clone)]
struct Session {
    #[serde(rename = "sessionID")]
    id: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct Compliance {
    //keep: bool,
    #[serde(rename = "templateCompliance")]
    compliance: String,
    #[serde(rename = "type")]
    profile_type: String,
    uuid: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct ComplianceResult {
    members: Vec<Compliance>,
}

pub async fn collect_hpe_metrics(settings: Console,  netbox: Netbox, interval_sec: u64, tx: mpsc::Sender<Node>) {
    info!("hpe client ready. interval: {}", interval_sec);
    let mut interval = interval(Duration::from_secs(interval_sec * 60));
    let mut host = settings.host.clone();
    host.set_path("rest/server-hardware");

    loop {
        interval.tick().await;
        info!("executing hpe metric collect");

        let token = match get_token(&settings).await {
            Ok(session) => session.id,
            Err(e) => {
                error!("could not get hpe api token: {}", e);
                continue;
            }
        };

        let resp = match get_request_builder(
            reqwest::Method::GET,
            Some(token.to_string()),
            None,
            host.to_owned(),
        )
        .send()
        .await
        {
            Ok(v) => v,
            Err(e) => {
                error!("error calling lxca: {}", e);
                continue;
            }
        };

        if resp.status() != reqwest::StatusCode::OK {
            error!("error api lxca code: {}", resp.status());
            continue
        }

        let json = match resp.json::<APIResponse>().await {
            Ok(parsed) => parsed,
            Err(e) => {
                error!("error parsing lxca json: {}", e);
                continue;
            }
        };

        for mut device in json.value.clone() {
            set_device_compliance_status(&settings, token.to_string(), &mut device)
                .await
                .unwrap_or_else(|error| {
                    error!("error checking device compliancy: {:?}", error);
                });
        }

        let mut nodes =  json.value.clone().into_iter().map(|d| Node::from(d)).collect::<Vec<Node>>();
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
                n.console = "oneview".to_string();
                tx.send(n.clone()).await.unwrap();
            } else {
                let n = Node { device_name: device.name, ..Default::default() };
                tx.send(n.clone()).await.unwrap();
            }
        }

        delete_token(&settings, token)
            .await
            .unwrap_or_else(|error| {
                error!("error deleting hpe token: {:?}", error);
            })
    }

    async fn set_device_compliance_status(
        settings: &Console,
        token: String,
        device: &mut Device,
    ) -> Result<(), reqwest::Error> {
        let mut host = settings.host.clone();
        host.set_path("rest/server-profiles");
        host.set_query(Some(format!("filter='uuid' = '{}'", device.uuid).as_str()));
        let json = get_request_builder(
            reqwest::Method::GET, 
            Some(token.to_string()),
            None,
            host
        )
            .send()
            .await?
            .error_for_status()?
            .json::<ComplianceResult>()
            .await?;

        if json.members.len() > 0 {
            device.compliant = json.members[0].compliance.to_string()
        }

        Ok(())
    }

    async fn get_token(settings: &Console) -> reqwest::Result<Session> {
        let mut host = settings.host.clone();
        host.set_path("rest/login-sessions");
        let username = &Some(settings.username.to_string());
        let mut auth = std::collections::HashMap::new();
        auth.insert("authLoginDomain", &settings.domain);
        auth.insert("userName", username);
        auth.insert("password", &settings.password);

        let sess = get_request_builder(
            reqwest::Method::POST, 
            None,
            None,
            host
        )
            .json(&auth)
            .send()
            .await?
            .error_for_status()?
            .json::<Session>()
            .await?;
        Ok(sess)
    }

    async fn delete_token(settings: &Console, token: String) -> Result<(), reqwest::Error> {
        let mut host = settings.host.clone();
        host.set_path("rest/login-sessions");
        get_request_builder(
            reqwest::Method::POST,
            Some(token), 
            None,
            host
        )
            .send()
            .await?;

        Ok(())
    }
}

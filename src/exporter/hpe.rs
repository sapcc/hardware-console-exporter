use log::{error, info};
use reqwest;
use reqwest::header::{ACCEPT, CONTENT_TYPE};
use tokio::time::{Duration, interval};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

use super::Console;
use super::Node;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Device {
    pub uuid: String,
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
            status,
            model: d.model,
            power_state: power,
            connection_state: 0,
            compliant: compliant,
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

pub async fn collect_hpe_metrics(settings: Console, interval_sec: u64, tx: mpsc::Sender<Node>) {
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

        if resp.status() == reqwest::StatusCode::OK {
            let json = match resp.json::<APIResponse>().await {
                Ok(parsed) => parsed,
                Err(e) => {
                    error!("error parsing lxca json: {}", e);
                    continue;
                }
            };

            for mut device in json.value {
                set_device_compliance_status(&settings, token.to_string(), &mut device)
                    .await
                    .unwrap_or_else(|error| {
                        error!("error checking device compliancy: {:?}", error);
                    });
                let node = Node::from(device);
                tx.send(node).await.unwrap_or_else(|error| {
                    error!("error sending node on channel: {:?}", error);
                })
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
        let json = get_request_builder(reqwest::Method::GET, Some(token.to_string()), host)
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

        let sess = get_request_builder(reqwest::Method::POST, None, host)
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
        get_request_builder(reqwest::Method::POST, Some(token), host)
            .send()
            .await?;

        Ok(())
    }

    fn get_request_builder(
        method: reqwest::Method,
        token: Option<String>,
        url: reqwest::Url,
    ) -> reqwest::RequestBuilder {
        let client = match Client::builder().danger_accept_invalid_certs(true).build() {
            Ok(client) => client,
            Err(error) => panic!("error creating reqwest client: {:?}", error),
        };
        let mut header_map = reqwest::header::HeaderMap::new();
        header_map.insert(CONTENT_TYPE, "application/json".parse().unwrap());
        header_map.insert(ACCEPT, "application/json".parse().unwrap());
        header_map.insert("X-Api-Version", "1400".parse().unwrap());
        if token.is_some() {
            header_map.insert("auth", token.unwrap().parse().unwrap());
        }

        return client.request(method, url).headers(header_map);
    }
}

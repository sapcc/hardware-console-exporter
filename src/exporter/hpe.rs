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
struct Device {
    #[serde(alias = "name")]
    pub device_name: String,
    pub model: String,
    pub status: String,
    #[serde(alias = "powerState")]
    pub power_state: String,
}

impl From<Device> for Node {
    fn from(d: Device) -> Self {
        let status = if d.status == "OK" { 1 } else { 0 };
        let power = if d.power_state == "On" { 1 } else { 0 };
        Self {
            device_name: d.device_name,
            status,
            model: d.model,
            power_state: power,
            connection_state: 0,
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

pub async fn collect_hpe_metrics(settings: Console, tx: mpsc::Sender<Node>) {
    let client = match Client::builder().danger_accept_invalid_certs(true).build() {
        Ok(client) => client,
        Err(error) => panic!("error creating reqwest client: {:?}", error),
    };
    info!("hpe client ready. interval: {}", settings.interval_in_min);
    let mut interval = time::interval(Duration::from_secs(settings.interval_in_min * 60));
    let mut host = settings.host.clone();
    host.set_path("rest/server-hardware");
    let url = host.as_str();

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

        let resp = match client
            .get(url)
            .header(CONTENT_TYPE, "application/json")
            .header(ACCEPT, "application/json")
            .header("auth", token)
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

    async fn get_token(settings: &Console) -> reqwest::Result<Session> {
        let client = match Client::builder().danger_accept_invalid_certs(true).build() {
            Ok(client) => client,
            Err(error) => panic!("error creating reqwest client: {:?}", error),
        };
        let mut host = settings.host.clone();
        host.set_path("rest/login-sessions");
        let username = &Some(settings.username.to_string());
        let mut auth = std::collections::HashMap::new();
        auth.insert("authLoginDomain", &settings.domain);
        auth.insert("userName", username);
        auth.insert("password", &settings.password);

        let sess = client
            .post(host)
            .header(CONTENT_TYPE, "application/json")
            .json(&auth)
            .send()
            .await?
            .error_for_status()?
            .json::<Session>()
            .await?;
        Ok(sess)
    }
}

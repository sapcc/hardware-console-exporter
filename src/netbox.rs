use url::Url;
use serde::{Deserialize, Serialize};
use reqwest::Client;
use reqwest::header::{ACCEPT, CONTENT_TYPE};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct NetboxDevice {
    pub id: u16,
    pub name: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct NetboxDeviceList {
    pub results: Vec<NetboxDevice>,
}

#[derive(Debug, Clone)]
pub struct Netbox {
    url: Url,
    query: String,
}


impl Netbox {
    pub fn new(url: Url, query: String) -> Netbox {
        Netbox {
            url: url,
            query: query,
        }
    }

    pub async fn get_devices_by_manufacturer(&self, manufacturer: String) -> reqwest::Result<Vec<NetboxDevice>> {
        let mut host = self.url.clone();
        let query = self.query.to_owned();
        host.set_path("/api/dcim/devices/");
        host.set_query(Some(format!("{query}&limit=2000&manufacturer={}", manufacturer).as_str()));
        let client = match Client::builder().danger_accept_invalid_certs(true).build() {
            Ok(client) => client,
            Err(error) => panic!("error creating reqwest client: {:?}", error),
        };
        let mut header_map = reqwest::header::HeaderMap::new();
        header_map.insert(CONTENT_TYPE, "application/json".parse().unwrap());
        header_map.insert(ACCEPT, "application/json".parse().unwrap());
        let result =  client.request(reqwest::Method::GET, host).headers(header_map)
            .send()
            .await?
            .error_for_status()?
            .json::<NetboxDeviceList>()
            .await?;

        Ok(result.results)
    }
}
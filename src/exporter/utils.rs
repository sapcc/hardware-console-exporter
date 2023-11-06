use reqwest::header::{ACCEPT, CONTENT_TYPE};
use serde::{Deserialize, Deserializer, de::Error};
use reqwest::Client;

use super::Console;


pub fn get_request_builder(
    method: reqwest::Method,
    token: Option<String>,
    settings: Option<&Console>,
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
        return client.request(method, url).headers(header_map);
    }
    if settings.is_some() {
        let settings = settings.unwrap();
        return client.request(method, url).headers(header_map)
        .basic_auth(settings.username.to_string(), settings.password.to_owned())
    }

    return client.request(method, url).headers(header_map);
}

pub fn deserialize_name<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    let buf = String::deserialize(deserializer)?;
    let name = buf.split(".").collect::<Vec<&str>>();
    if name.len() == 0 {
        return Err(D::Error::custom("invalid compliance name"));
    }
    let name = name[0].to_string().replace("r", "");
    Ok(name)
}
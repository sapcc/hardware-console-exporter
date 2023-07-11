use config::{Config, ConfigError, Environment, File};
use serde::{de::Error, Deserialize, Deserializer};
use std::env;
use url::Url;

#[derive(Debug, Deserialize, Clone)]
#[allow(unused)]
pub struct Console {
    #[serde(deserialize_with = "de_url")]
    pub host: Url,
    pub domain: Option<String>,
    pub username: String,
    pub password: Option<String>,
    #[serde(default = "default_policy_name")]
    pub policy_name: String,
}

#[derive(Debug, Deserialize, Clone)]
#[allow(unused)]
pub struct Settings {
    pub dell: Console,
    pub lenovo: Console,
    pub hpe: Console,
    #[serde(default = "default_interval")]
    pub interval_in_min: u64,
}

fn default_interval() -> u64 {
    30
}

fn default_policy_name() -> String {
    "".to_string()
}

fn de_url<'de, D>(deserializer: D) -> Result<Url, D::Error>
where
    D: Deserializer<'de>,
{
    let buf = String::deserialize(deserializer)?;
    Url::parse(&buf).map_err(D::Error::custom)
}

impl Settings {
    pub fn new() -> Result<Self, ConfigError> {
        let run_mode = env::var("RUN_MODE").unwrap_or_else(|_| "development".into());

        let s = Config::builder()
            .add_source(File::with_name("etc/default.yaml"))
            .add_source(File::with_name(&format!("etc/{}", run_mode)).required(false))
            .add_source(Environment::with_prefix("exporter").separator("_"))
            .build()?;

        // You can deserialize (and thus freeze) the entire configuration as
        s.try_deserialize()
    }
}

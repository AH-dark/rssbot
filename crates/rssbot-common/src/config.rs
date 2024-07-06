use serde::Deserialize;

#[derive(Debug, Deserialize, Default)]
pub struct Config {
    pub database_url: String,
    pub redis_url: String,

    #[serde(default = "Config::default_otel_exporter_endpoint")]
    pub otel_exporter_endpoint: String,
    #[serde(default)]
    pub otel_exporter: OtelExporter,
    #[serde(default = "Config::default_otel_sample_rate")]
    pub otel_sample_rate: f64,

    #[serde(default = "Config::default_api_server")]
    pub api_server: String,
    pub bot_token: String,

    #[serde(default = "Config::default_webhook_address")]
    pub webhook_address: String,
    pub webhook_url: String,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum OtelExporter {
    #[default]
    OtlpGrpc,
    OtlpHttp,
}

impl Config {
    pub fn new() -> envy::Result<Self> {
        let config = envy::from_env::<Config>()?;
        Ok(config)
    }

    fn default_otel_exporter_endpoint() -> String {
        "http://localhost:4317".into()
    }

    fn default_otel_sample_rate() -> f64 {
        1.0
    }

    fn default_api_server() -> String {
        "https://api.telegram.org".into()
    }

    fn default_webhook_address() -> String {
        "0.0.0.0:8080".into()
    }
}

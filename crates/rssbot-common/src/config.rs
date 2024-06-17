use serde::Deserialize;

#[derive(Debug, Deserialize, Default)]
pub struct Config {
    pub database_url: String,
    #[serde(default = "default_otel_exporter")]
    pub otel_exporter_endpoint: String,
    pub otel_exporter: OtelExporter,
    #[serde(default = "default_otel_sample_rate")]
    pub otel_sample_rate: f64,
}

fn default_otel_exporter() -> String {
    "http://localhost:4317".into()
}

fn default_otel_sample_rate() -> f64 {
    1.0
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
}

use opentelemetry::global;
use opentelemetry_otlp::{
    ExportConfig, HttpExporterBuilder, SpanExporterBuilder, TonicExporterBuilder, WithExportConfig,
};
use opentelemetry_sdk::propagation::TraceContextPropagator;
use opentelemetry_sdk::runtime::Tokio;
use opentelemetry_sdk::trace::Sampler;
use tracing_subscriber::{EnvFilter, Registry};
use tracing_subscriber::layer::SubscriberExt;

use crate::config::{Config, OtelExporter};
use crate::observability::resource::init_resource;

pub fn init_tracer(service_name: String, service_version: String, config: &Config) {
    let export_config = ExportConfig {
        endpoint: config.otel_exporter_endpoint.to_string(),
        ..Default::default()
    };

    let exporter = match config.otel_exporter
    {
        OtelExporter::OtlpHttp => SpanExporterBuilder::Http(
            HttpExporterBuilder::default().with_export_config(export_config),
        ),
        OtelExporter::OtlpGrpc => SpanExporterBuilder::Tonic(
            TonicExporterBuilder::default().with_export_config(export_config),
        ),
    };

    global::set_text_map_propagator(TraceContextPropagator::new());

    let tracer = opentelemetry_otlp::new_pipeline()
        .tracing()
        .with_exporter(exporter)
        .with_trace_config(
            opentelemetry_sdk::trace::config()
                .with_sampler(Sampler::TraceIdRatioBased(config.otel_sample_rate))
                .with_resource(init_resource(service_name, service_version)),
        )
        .install_batch(Tokio)
        .expect("Failed to install `opentelemetry` tracer.");

    let telemetry = tracing_opentelemetry::layer().with_tracer(tracer);
    let env_filter = EnvFilter::try_from_default_env().unwrap_or(EnvFilter::new("INFO"));
    let subscriber = Registry::default().with(telemetry).with(env_filter);
    tracing::subscriber::set_global_default(subscriber)
        .expect("Failed to install `tracing` subscriber.");
}

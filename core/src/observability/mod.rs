//! Observability module for metrics and tracing
//!
//! Provides OpenTelemetry integration for job execution metrics,
//! distributed tracing, and operational visibility.

use opentelemetry::metrics::{Counter, Histogram, Meter, MeterProvider};
use opentelemetry::KeyValue;
use opentelemetry_sdk::metrics::SdkMeterProvider;
use std::sync::OnceLock;
use std::time::Instant;

/// Global metrics instance
static METRICS: OnceLock<Metrics> = OnceLock::new();

/// Virtues metrics for job execution and data pipeline
pub struct Metrics {
    /// Total jobs started by type
    pub jobs_started: Counter<u64>,
    /// Total jobs completed successfully by type
    pub jobs_succeeded: Counter<u64>,
    /// Total jobs failed by type
    pub jobs_failed: Counter<u64>,
    /// Job duration in seconds
    pub job_duration_seconds: Histogram<f64>,
    /// Records synced per job
    pub records_synced: Counter<u64>,
    /// Records transformed per job
    pub records_transformed: Counter<u64>,
    /// S3 upload bytes
    pub s3_upload_bytes: Counter<u64>,
    /// S3 upload duration in seconds
    pub s3_upload_duration_seconds: Histogram<f64>,
}

impl Metrics {
    /// Create metrics from a meter
    pub fn new(meter: &Meter) -> Self {
        Self {
            jobs_started: meter
                .u64_counter("virtues_jobs_started_total")
                .with_description("Total number of jobs started")
                .with_unit("jobs")
                .build(),
            jobs_succeeded: meter
                .u64_counter("virtues_jobs_succeeded_total")
                .with_description("Total number of jobs completed successfully")
                .with_unit("jobs")
                .build(),
            jobs_failed: meter
                .u64_counter("virtues_jobs_failed_total")
                .with_description("Total number of jobs that failed")
                .with_unit("jobs")
                .build(),
            job_duration_seconds: meter
                .f64_histogram("virtues_job_duration_seconds")
                .with_description("Duration of job execution")
                .with_unit("s")
                .build(),
            records_synced: meter
                .u64_counter("virtues_records_synced_total")
                .with_description("Total number of records synced")
                .with_unit("records")
                .build(),
            records_transformed: meter
                .u64_counter("virtues_records_transformed_total")
                .with_description("Total number of records transformed")
                .with_unit("records")
                .build(),
            s3_upload_bytes: meter
                .u64_counter("virtues_s3_upload_bytes_total")
                .with_description("Total bytes uploaded to S3")
                .with_unit("bytes")
                .build(),
            s3_upload_duration_seconds: meter
                .f64_histogram("virtues_s3_upload_duration_seconds")
                .with_description("Duration of S3 uploads")
                .with_unit("s")
                .build(),
        }
    }

    /// Record job started
    pub fn record_job_started(&self, job_type: &str) {
        self.jobs_started
            .add(1, &[KeyValue::new("job_type", job_type.to_string())]);
    }

    /// Record job succeeded with duration
    pub fn record_job_succeeded(&self, job_type: &str, duration: f64) {
        let attrs = &[KeyValue::new("job_type", job_type.to_string())];
        self.jobs_succeeded.add(1, attrs);
        self.job_duration_seconds.record(duration, attrs);
    }

    /// Record job failed with duration
    pub fn record_job_failed(&self, job_type: &str, duration: f64) {
        let attrs = &[KeyValue::new("job_type", job_type.to_string())];
        self.jobs_failed.add(1, attrs);
        self.job_duration_seconds.record(duration, attrs);
    }

    /// Record records synced
    pub fn record_records_synced(&self, count: u64, stream_name: &str) {
        self.records_synced.add(
            count,
            &[KeyValue::new("stream_name", stream_name.to_string())],
        );
    }

    /// Record records transformed
    pub fn record_records_transformed(&self, count: u64, target_table: &str) {
        self.records_transformed.add(
            count,
            &[KeyValue::new("target_table", target_table.to_string())],
        );
    }

    /// Record S3 upload
    pub fn record_s3_upload(&self, bytes: u64, duration: f64) {
        self.s3_upload_bytes.add(bytes, &[]);
        self.s3_upload_duration_seconds.record(duration, &[]);
    }
}

/// Configuration for observability
#[derive(Debug, Clone)]
pub struct ObservabilityConfig {
    /// OTLP endpoint (e.g., "http://localhost:4317")
    pub otlp_endpoint: Option<String>,
    /// Service name for tracing
    pub service_name: String,
}

impl Default for ObservabilityConfig {
    fn default() -> Self {
        Self {
            otlp_endpoint: std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT").ok(),
            service_name: "virtues".to_string(),
        }
    }
}

/// Initialize observability with optional OTLP export
///
/// If OTEL_EXPORTER_OTLP_ENDPOINT is set, metrics will be exported to that endpoint.
/// Otherwise, metrics are still collected but only logged locally.
pub fn init(config: ObservabilityConfig) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let meter_provider = if let Some(endpoint) = &config.otlp_endpoint {
        // Configure OTLP exporter
        use opentelemetry_otlp::WithExportConfig;
        use opentelemetry_sdk::metrics::PeriodicReader;
        use opentelemetry_sdk::runtime;

        let exporter = opentelemetry_otlp::MetricExporter::builder()
            .with_tonic()
            .with_endpoint(endpoint)
            .build()?;

        let reader = PeriodicReader::builder(exporter, runtime::Tokio)
            .with_interval(std::time::Duration::from_secs(30))
            .build();

        SdkMeterProvider::builder()
            .with_reader(reader)
            .build()
    } else {
        // No OTLP endpoint - use noop provider (metrics still tracked in-memory)
        tracing::info!("OTEL_EXPORTER_OTLP_ENDPOINT not set, metrics will be logged only");
        SdkMeterProvider::builder().build()
    };

    // Note: meter() requires &'static str, so we use the default name
    let meter = meter_provider.meter("virtues");
    let metrics = Metrics::new(&meter);

    // Store globally
    METRICS
        .set(metrics)
        .map_err(|_| "Metrics already initialized")?;

    tracing::info!(
        otlp_endpoint = ?config.otlp_endpoint,
        "Observability initialized"
    );

    Ok(())
}

/// Get global metrics instance
///
/// Returns None if observability has not been initialized.
/// Use `init()` at startup to initialize.
pub fn metrics() -> Option<&'static Metrics> {
    METRICS.get()
}

/// Helper to time an operation and record metrics
pub struct JobTimer {
    job_type: String,
    start: Instant,
}

impl JobTimer {
    /// Start timing a job
    pub fn start(job_type: &str) -> Self {
        if let Some(m) = metrics() {
            m.record_job_started(job_type);
        }
        Self {
            job_type: job_type.to_string(),
            start: Instant::now(),
        }
    }

    /// Record job success
    pub fn success(self) {
        let duration = self.start.elapsed().as_secs_f64();
        if let Some(m) = metrics() {
            m.record_job_succeeded(&self.job_type, duration);
        }
        tracing::info!(
            job_type = %self.job_type,
            duration_seconds = duration,
            "Job completed successfully"
        );
    }

    /// Record job failure
    pub fn failure(self, error: &str) {
        let duration = self.start.elapsed().as_secs_f64();
        if let Some(m) = metrics() {
            m.record_job_failed(&self.job_type, duration);
        }
        tracing::error!(
            job_type = %self.job_type,
            duration_seconds = duration,
            error = %error,
            "Job failed"
        );
    }

    /// Get elapsed duration without consuming timer
    pub fn elapsed(&self) -> f64 {
        self.start.elapsed().as_secs_f64()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_job_timer() {
        let timer = JobTimer::start("test");
        std::thread::sleep(std::time::Duration::from_millis(10));
        assert!(timer.elapsed() >= 0.01);
    }
}

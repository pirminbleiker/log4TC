//! Log dispatcher - routes logs to OTEL exporter and configured outputs

use anyhow::Result;
use log4tc_core::{AppSettings, LogEntry, LogRecord, MessageFormatter};
use log4tc_otel::OtelExporter;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Output plugin trait for implementing custom output handlers
#[async_trait::async_trait]
pub trait OutputPlugin: Send + Sync {
    async fn handle(&self, record: LogRecord) -> Result<()>;
    fn name(&self) -> &str;
}

/// OTEL export plugin - sends logs to collector via OTLP
struct OtelOutputPlugin {
    exporter: OtelExporter,
}

#[async_trait::async_trait]
impl OutputPlugin for OtelOutputPlugin {
    async fn handle(&self, record: LogRecord) -> Result<()> {
        self.exporter.export(record).await
            .map_err(|e| anyhow::anyhow!("OTEL export error: {}", e))
    }

    fn name(&self) -> &str {
        "otel"
    }
}

/// Console logging output plugin (for debugging)
struct ConsoleLogOutput;

#[async_trait::async_trait]
impl OutputPlugin for ConsoleLogOutput {
    async fn handle(&self, record: LogRecord) -> Result<()> {
        tracing::info!(
            "[{}] {} {}",
            record.severity_text,
            record.scope_attributes.get("logger.name")
                .and_then(|v| v.as_str())
                .unwrap_or("?"),
            record.body
        );
        Ok(())
    }

    fn name(&self) -> &str {
        "console"
    }
}

/// Log dispatcher routes incoming logs to OTEL + configured outputs
#[derive(Clone)]
pub struct LogDispatcher {
    outputs: Arc<RwLock<Vec<Arc<dyn OutputPlugin>>>>,
    channel_capacity: usize,
}

impl LogDispatcher {
    pub async fn new(settings: &AppSettings) -> Result<Self> {
        let mut outputs: Vec<Arc<dyn OutputPlugin>> = Vec::new();

        // Always add OTEL exporter
        let otel_endpoint = std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT")
            .unwrap_or_else(|_| "http://otel-collector:4318/v1/logs".to_string());
        let exporter = OtelExporter::new(otel_endpoint, 100, 3);
        outputs.push(Arc::new(OtelOutputPlugin { exporter }));
        tracing::info!("OTEL exporter configured");

        // Always add console output
        outputs.push(Arc::new(ConsoleLogOutput));

        Ok(Self {
            outputs: Arc::new(RwLock::new(outputs)),
            channel_capacity: settings.service.channel_capacity,
        })
    }

    /// Dispatch a log entry to all configured outputs
    pub async fn dispatch(&self, entry: LogEntry) -> Result<()> {
        tracing::trace!("Dispatching log entry: {}", entry.id);

        // Format message with arguments
        let formatted_message = MessageFormatter::format_with_context(
            &entry.message,
            &entry.arguments,
            &entry.context,
        );

        // Convert to LogRecord for output plugins
        let mut record = LogRecord::from_log_entry(entry);
        record.body = serde_json::json!(formatted_message);

        // Send to all configured outputs
        let outputs = self.outputs.read().await;

        // Collect errors but continue dispatching to all outputs
        let mut errors = Vec::new();

        for output in outputs.iter() {
            if let Err(e) = output.handle(record.clone()).await {
                tracing::error!("Output plugin {} error: {}", output.name(), e);
                errors.push(e);
            }
        }

        if !errors.is_empty() {
            tracing::warn!(
                "Dispatcher had {} errors while dispatching log",
                errors.len()
            );
        }

        Ok(())
    }

    /// Get the current number of configured outputs
    pub async fn output_count(&self) -> usize {
        self.outputs.read().await.len()
    }
}

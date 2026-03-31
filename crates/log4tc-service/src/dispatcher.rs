//! Log dispatcher - routes logs to configured outputs with backpressure handling

use anyhow::Result;
use log4tc_core::{AppSettings, LogEntry, LogRecord, MessageFormatter};
use std::sync::Arc;
use tokio::sync::RwLock;

/// Output plugin trait for implementing custom output handlers
#[async_trait::async_trait]
pub trait OutputPlugin: Send + Sync {
    /// Handle a log record asynchronously
    async fn handle(&self, record: LogRecord) -> Result<()>;

    /// Get the plugin name
    fn name(&self) -> &str;
}

/// Default logging output plugin
struct DefaultLogOutput;

#[async_trait::async_trait]
impl OutputPlugin for DefaultLogOutput {
    async fn handle(&self, record: LogRecord) -> Result<()> {
        tracing::info!(
            severity = record.severity_text,
            body = ?record.body,
            "Log output"
        );
        Ok(())
    }

    fn name(&self) -> &str {
        "default"
    }
}

/// Log dispatcher routes incoming logs to configured outputs
#[derive(Clone)]
pub struct LogDispatcher {
    outputs: Arc<RwLock<Vec<Arc<dyn OutputPlugin>>>>,
    channel_capacity: usize,
}

impl LogDispatcher {
    /// Create a new log dispatcher
    pub async fn new(settings: &AppSettings) -> Result<Self> {
        tracing::info!("Initializing log dispatcher with {} outputs", settings.outputs.len());

        let mut outputs: Vec<Arc<dyn OutputPlugin>> = Vec::new();

        // TODO: Load and initialize actual output plugins based on configuration
        // For now, add default output
        if settings.outputs.is_empty() {
            outputs.push(Arc::new(DefaultLogOutput));
            tracing::info!("No outputs configured, using default logging output");
        }

        for output_config in &settings.outputs {
            tracing::debug!("Configuring output: {}", output_config.output_type);
            // TODO: Factory pattern to create outputs based on type
            // outputs.push(Arc::new(create_output_plugin(&output_config)?));
        }

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

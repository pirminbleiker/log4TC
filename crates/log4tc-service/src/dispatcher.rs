//! Log dispatcher with batched async export to Victoria-Logs
//!
//! Logs are collected in a batch buffer and flushed either when the batch
//! is full or after a timeout - whichever comes first. This minimizes
//! HTTP overhead and CPU usage.

use anyhow::Result;
use log4tc_core::{AppSettings, LogEntry, LogRecord, MessageFormatter};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;

// Defaults moved to config.rs ExportConfig

/// Log dispatcher - converts LogEntries and sends them to a batched export worker
#[derive(Clone)]
pub struct LogDispatcher {
    export_tx: mpsc::Sender<LogRecord>,
}

impl LogDispatcher {
    pub async fn new(settings: &AppSettings) -> Result<Self> {
        // ENV override for endpoint, otherwise use config
        let endpoint = std::env::var("LOG4TC_EXPORT_ENDPOINT")
            .unwrap_or_else(|_| settings.export.endpoint.clone());

        let batch_size = settings.export.batch_size;
        let flush_interval = Duration::from_millis(settings.export.flush_interval_ms);

        // Bounded channel for backpressure
        let (export_tx, export_rx) = mpsc::channel::<LogRecord>(settings.service.channel_capacity);

        // Spawn background batch worker
        tokio::spawn(Self::batch_worker(export_rx, endpoint, batch_size, flush_interval));

        tracing::info!("Dispatcher ready (batch={}, flush={}ms)", batch_size, flush_interval.as_millis());

        Ok(Self { export_tx })
    }

    /// Dispatch a log entry - formats and sends to export worker (non-blocking)
    pub async fn dispatch(&self, entry: LogEntry) -> Result<()> {
        // Format message only if template has placeholders
        let body = if entry.message.contains('{') {
            MessageFormatter::format_with_context(&entry.message, &entry.arguments, &entry.context)
        } else {
            entry.message.clone()
        };

        let mut record = LogRecord::from_log_entry(entry);
        record.body = serde_json::Value::String(body);

        // Non-blocking send - drops if channel full (backpressure)
        if self.export_tx.try_send(record).is_err() {
            tracing::warn!("Export channel full, dropping log");
        }

        Ok(())
    }

    /// Background worker that batches records and flushes to endpoint
    async fn batch_worker(
        mut rx: mpsc::Receiver<LogRecord>,
        endpoint: String,
        batch_size: usize,
        flush_interval: Duration,
    ) {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());

        let mut batch: Vec<LogRecord> = Vec::with_capacity(batch_size);
        let mut interval = tokio::time::interval(flush_interval);
        let mut total_sent: u64 = 0;
        let mut total_errors: u64 = 0;

        loop {
            tokio::select! {
                // Receive new record
                Some(record) = rx.recv() => {
                    batch.push(record);
                    if batch.len() >= batch_size {
                        match Self::flush_batch(&client, &endpoint, &batch).await {
                            Ok(n) => total_sent += n as u64,
                            Err(e) => {
                                total_errors += 1;
                                tracing::error!("Batch export error: {}", e);
                            }
                        }
                        batch.clear();
                    }
                }
                // Periodic flush
                _ = interval.tick() => {
                    if !batch.is_empty() {
                        match Self::flush_batch(&client, &endpoint, &batch).await {
                            Ok(n) => total_sent += n as u64,
                            Err(e) => {
                                total_errors += 1;
                                tracing::error!("Batch export error: {}", e);
                            }
                        }
                        batch.clear();
                    }
                }
                // Channel closed
                else => {
                    // Flush remaining
                    if !batch.is_empty() {
                        let _ = Self::flush_batch(&client, &endpoint, &batch).await;
                    }
                    tracing::info!("Export worker stopped. Total sent: {}, errors: {}", total_sent, total_errors);
                    break;
                }
            }
        }
    }

    /// Flush a batch of records to the endpoint as JSONL
    async fn flush_batch(
        client: &reqwest::Client,
        endpoint: &str,
        batch: &[LogRecord],
    ) -> Result<usize> {
        let count = batch.len();

        // Build JSONL payload (one JSON object per line)
        let mut payload = String::with_capacity(count * 256);
        for record in batch {
            let mut obj = serde_json::Map::new();

            // Standard fields - use PLC timestamp as _time for correct ordering
            obj.insert("_msg".to_string(), record.body.clone());
            obj.insert("_time".to_string(), serde_json::json!(record.timestamp.to_rfc3339()));
            obj.insert("level".to_string(), serde_json::json!(record.severity_text.to_lowercase()));
            obj.insert("severity_number".to_string(), serde_json::json!(record.severity_number));

            // Scope attributes (logger name)
            for (k, v) in &record.scope_attributes {
                obj.insert(k.clone(), v.clone());
            }

            // Resource attributes (service, host, task)
            for (k, v) in &record.resource_attributes {
                obj.insert(k.clone(), v.clone());
            }

            // Log attributes (context, args, plc metadata)
            for (k, v) in &record.log_attributes {
                obj.insert(k.clone(), v.clone());
            }

            payload.push_str(&serde_json::to_string(&serde_json::Value::Object(obj))?);
            payload.push('\n');
        }

        let response = client
            .post(endpoint)
            .header("Content-Type", "application/stream+json")
            .body(payload)
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("HTTP error: {}", e))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!("Export failed: HTTP {} - {}", status, body));
        }

        tracing::debug!("Exported {} logs to {}", count, endpoint);
        Ok(count)
    }
}

//! OpenTelemetry (OTEL) OTLP receiver and exporter for Log4TC
//!
//! This crate implements the OTEL protocol endpoints for receiving log data
//! from TwinCAT PLCs and exporting processed logs to OTEL collectors.
//!
//! Supports:
//! - OTLP HTTP/JSON endpoint (POST /v1/logs)
//! - OTLP gRPC endpoint (4317)
//! - Log record transformation to OTEL LogRecord format
//! - Batching and retry logic for exports

pub mod receiver;
pub mod exporter;
pub mod error;
pub mod mapping;

pub use receiver::{OtelHttpReceiver, OtelGrpcReceiver};
pub use exporter::OtelExporter;
pub use error::{Result, OtelError};
pub use mapping::OtelMapping;

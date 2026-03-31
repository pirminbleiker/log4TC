//! Core data models for Log4TC

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// Log severity level, mapped from ADS binary protocol
/// Values match the .NET Log4Tc.Model.LogLevel enumeration
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, PartialOrd, Ord, Hash)]
pub enum LogLevel {
    Trace = 0,
    Debug = 1,
    Info = 2,
    Warn = 3,
    Error = 4,
    Fatal = 5,
}

impl LogLevel {
    pub fn as_u8(&self) -> u8 {
        *self as u8
    }

    pub fn from_u8(val: u8) -> Option<Self> {
        match val {
            0 => Some(LogLevel::Trace),
            1 => Some(LogLevel::Debug),
            2 => Some(LogLevel::Info),
            3 => Some(LogLevel::Warn),
            4 => Some(LogLevel::Error),
            5 => Some(LogLevel::Fatal),
            _ => None,
        }
    }

    /// Convert LogLevel to OpenTelemetry SeverityNumber
    /// Mapping: Trace->1, Debug->5, Info->9, Warn->13, Error->17, Fatal->21
    pub fn to_otel_severity_number(&self) -> i32 {
        match self {
            LogLevel::Trace => 1,
            LogLevel::Debug => 5,
            LogLevel::Info => 9,
            LogLevel::Warn => 13,
            LogLevel::Error => 17,
            LogLevel::Fatal => 21,
        }
    }

    /// Convert LogLevel to OpenTelemetry SeverityText
    pub fn to_otel_severity_text(&self) -> &'static str {
        match self {
            LogLevel::Trace => "TRACE",
            LogLevel::Debug => "DEBUG",
            LogLevel::Info => "INFO",
            LogLevel::Warn => "WARN",
            LogLevel::Error => "ERROR",
            LogLevel::Fatal => "FATAL",
        }
    }
}

impl std::fmt::Display for LogLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LogLevel::Trace => write!(f, "Trace"),
            LogLevel::Debug => write!(f, "Debug"),
            LogLevel::Info => write!(f, "Info"),
            LogLevel::Warn => write!(f, "Warn"),
            LogLevel::Error => write!(f, "Error"),
            LogLevel::Fatal => write!(f, "Fatal"),
        }
    }
}

/// A log entry from the ADS protocol or OTEL receiver
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    pub id: String,

    // Source identification
    pub source: String,      // AMS address or source identifier
    pub hostname: String,    // PLC hostname

    // Message content
    pub message: String,     // Template string or formatted message
    pub logger: String,      // Logger name

    pub level: LogLevel,     // Severity level

    // Timestamps
    pub plc_timestamp: DateTime<Utc>,     // PLC-side time
    pub clock_timestamp: DateTime<Utc>,   // System clock time

    // Task metadata
    pub task_index: i32,             // Task ID
    pub task_name: String,           // Task name
    pub task_cycle_counter: u32,     // Cycle count

    // Application metadata
    pub app_name: String,            // Application name
    pub project_name: String,        // Project name
    pub online_change_count: u32,    // Online changes

    // Variable data
    pub arguments: HashMap<usize, serde_json::Value>,  // Positional arguments
    pub context: HashMap<String, serde_json::Value>,   // Context properties
}

impl LogEntry {
    pub fn new(
        source: String,
        hostname: String,
        message: String,
        logger: String,
        level: LogLevel,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            source,
            hostname,
            message,
            logger,
            level,
            plc_timestamp: Utc::now(),
            clock_timestamp: Utc::now(),
            task_index: 0,
            task_name: String::new(),
            task_cycle_counter: 0,
            app_name: String::new(),
            project_name: String::new(),
            online_change_count: 0,
            arguments: HashMap::new(),
            context: HashMap::new(),
        }
    }
}

/// OpenTelemetry LogRecord representation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogRecord {
    pub timestamp: DateTime<Utc>,
    pub body: serde_json::Value,
    pub severity_number: i32,
    pub severity_text: String,
    pub resource_attributes: HashMap<String, serde_json::Value>,
    pub scope_attributes: HashMap<String, serde_json::Value>,
    pub log_attributes: HashMap<String, serde_json::Value>,
}

impl LogRecord {
    pub fn from_log_entry(entry: LogEntry) -> Self {
        let severity_number = entry.level.to_otel_severity_number();
        let severity_text = entry.level.to_otel_severity_text().to_string();

        // Pre-allocate resource attributes with expected capacity
        let mut resource_attributes = HashMap::with_capacity(5);
        resource_attributes.insert(
            "service.name".to_string(),
            serde_json::Value::String(entry.project_name),
        );
        resource_attributes.insert(
            "service.instance.id".to_string(),
            serde_json::Value::String(entry.app_name),
        );
        resource_attributes.insert(
            "host.name".to_string(),
            serde_json::Value::String(entry.hostname),
        );
        resource_attributes.insert(
            "process.pid".to_string(),
            serde_json::Value::Number(entry.task_index.into()),
        );
        resource_attributes.insert(
            "process.command_line".to_string(),
            serde_json::Value::String(entry.task_name),
        );

        let mut scope_attributes = HashMap::with_capacity(1);
        scope_attributes.insert(
            "logger.name".to_string(),
            serde_json::Value::String(entry.logger),
        );

        // Pre-allocate log_attributes: context items + 4 standard keys + arguments
        let expected_capacity = entry.context.len() + entry.arguments.len() + 4;
        let mut log_attributes = HashMap::with_capacity(expected_capacity);

        // Merge context items without cloning the entire map
        log_attributes.extend(entry.context);

        // Add standard OTEL attributes
        log_attributes.insert(
            "plc.timestamp".to_string(),
            serde_json::Value::String(entry.plc_timestamp.to_rfc3339()),
        );
        log_attributes.insert(
            "task.cycle".to_string(),
            serde_json::Value::Number(entry.task_cycle_counter.into()),
        );
        log_attributes.insert(
            "online.changes".to_string(),
            serde_json::Value::Number(entry.online_change_count.into()),
        );
        log_attributes.insert(
            "source.address".to_string(),
            serde_json::Value::String(entry.source),
        );

        // Merge in positional arguments with pre-formatted keys
        for (idx, val) in entry.arguments {
            log_attributes.insert(format!("arg.{}", idx), val);
        }

        Self {
            timestamp: entry.clock_timestamp,
            body: serde_json::Value::String(entry.message),
            severity_number,
            severity_text,
            resource_attributes,
            scope_attributes,
            log_attributes,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_log_level_conversion() {
        assert_eq!(LogLevel::from_u8(0), Some(LogLevel::Trace));
        assert_eq!(LogLevel::from_u8(2), Some(LogLevel::Info));
        assert_eq!(LogLevel::from_u8(4), Some(LogLevel::Error));
        assert_eq!(LogLevel::from_u8(255), None);
    }

    #[test]
    fn test_log_level_display() {
        assert_eq!(LogLevel::Trace.to_string(), "Trace");
        assert_eq!(LogLevel::Debug.to_string(), "Debug");
        assert_eq!(LogLevel::Info.to_string(), "Info");
        assert_eq!(LogLevel::Warn.to_string(), "Warn");
        assert_eq!(LogLevel::Error.to_string(), "Error");
        assert_eq!(LogLevel::Fatal.to_string(), "Fatal");
    }

    #[test]
    fn test_log_level_otel_severity() {
        assert_eq!(LogLevel::Trace.to_otel_severity_number(), 1);
        assert_eq!(LogLevel::Info.to_otel_severity_number(), 9);
        assert_eq!(LogLevel::Warn.to_otel_severity_number(), 13);
        assert_eq!(LogLevel::Fatal.to_otel_severity_number(), 21);

        assert_eq!(LogLevel::Trace.to_otel_severity_text(), "TRACE");
        assert_eq!(LogLevel::Fatal.to_otel_severity_text(), "FATAL");
    }

    #[test]
    fn test_log_entry_creation() {
        let entry = LogEntry::new(
            "192.168.1.1".to_string(),
            "plc-01".to_string(),
            "Test message".to_string(),
            "test.logger".to_string(),
            LogLevel::Info,
        );

        assert_eq!(entry.source, "192.168.1.1");
        assert_eq!(entry.level, LogLevel::Info);
        assert!(!entry.id.is_empty());
    }

    #[test]
    fn test_log_record_from_entry() {
        let mut entry = LogEntry::new(
            "192.168.1.1".to_string(),
            "plc-01".to_string(),
            "Test message".to_string(),
            "test.logger".to_string(),
            LogLevel::Warn,
        );
        entry.project_name = "TestProject".to_string();
        entry.app_name = "TestApp".to_string();

        let record = LogRecord::from_log_entry(entry);

        // Warn (3) maps to OTEL severity 13
        assert_eq!(record.severity_number, 13);
        assert_eq!(
            record.resource_attributes.get("service.name"),
            Some(&serde_json::Value::String("TestProject".to_string()))
        );
    }
}

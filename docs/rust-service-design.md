# log4TC Rust Service Design Specification

## 1. Overview

The Rust service is a complete rewrite of the log4TC .NET layer, designed to bridge Beckhoff TwinCAT PLCs and cloud-native observability backends. The service listens for binary log messages via the ADS (Automation Device Specification) protocol on port 16150, parses them according to the ADS binary protocol version 1, and exports the structured log entries to OpenTelemetry Logs (OTLP) via gRPC or HTTP.

**Key objectives:**
- Replace the .NET Framework with native Rust for improved performance and reduced memory footprint
- Standardize on OpenTelemetry Logs (OTLP) as the single output protocol
- Maintain full backward compatibility with existing TwinCAT PLC binary message format (protocol v1)
- Support configurable message filtering and formatting
- Provide Windows service integration with graceful lifecycle management
- Enable zero-copy parsing where possible and efficient batch processing

## 2. Workspace Layout

The project uses a Cargo workspace with four interdependent crates:

```
log4tc-rust/
├── Cargo.toml                      # Workspace root
├── crates/
│   ├── log4tc-ads/
│   │   ├── Cargo.toml
│   │   ├── src/
│   │   │   ├── lib.rs              # Public API for ADS protocol
│   │   │   ├── receiver.rs         # TCP listener on port 16150
│   │   │   ├── parser.rs           # Binary protocol v1 parser
│   │   │   ├── connection.rs       # Connection handler state machine
│   │   │   └── error.rs            # ADS-specific error types
│   │   └── tests/
│   │
│   ├── log4tc-core/
│   │   ├── Cargo.toml
│   │   ├── src/
│   │   │   ├── lib.rs              # Public API for core data model
│   │   │   ├── log_entry.rs        # LogEntry struct (44 fields)
│   │   │   ├── log_level.rs        # LogLevel enum
│   │   │   ├── message.rs          # Message formatting and template parsing
│   │   │   ├── filter.rs           # Log filtering trait and implementations
│   │   │   ├── context.rs          # Context data structures
│   │   │   ├── error.rs            # Core error types
│   │   │   └── types.rs            # Shared types (object enum, argument map)
│   │   └── tests/
│   │
│   ├── log4tc-otel/
│   │   ├── Cargo.toml
│   │   ├── src/
│   │   │   ├── lib.rs              # Public API for OTEL integration
│   │   │   ├── exporter.rs         # OTLP exporter trait
│   │   │   ├── grpc_exporter.rs    # gRPC-based OTLP exporter
│   │   │   ├── http_exporter.rs    # HTTP-based OTLP exporter
│   │   │   ├── mapping.rs          # LogEntry → OpenTelemetry LogRecord mapping
│   │   │   ├── batching.rs         # Batch collection and flushing
│   │   │   └── error.rs            # OTEL-specific error types
│   │   └── tests/
│   │
│   └── log4tc-service/
│       ├── Cargo.toml
│       ├── src/
│       │   ├── main.rs             # Entry point
│       │   ├── config.rs           # Configuration loading and validation
│       │   ├── service.rs          # Windows service lifecycle
│       │   ├── dispatcher.rs       # Log dispatcher (tokio channels)
│       │   ├── shutdown.rs         # Graceful shutdown handling
│       │   └── error.rs            # Application error types
│       └── tests/
│
└── README.md
```

## 3. Core Data Model

### 3.1 LogEntry Struct

The Rust `LogEntry` replicates all 44 fields from the .NET equivalent:

```rust
/// Complete log entry from TwinCAT PLC
#[derive(Debug, Clone)]
pub struct LogEntry {
    // Source identification (6 fields)
    pub source: String,                    // AMS address (e.g., "10.10.1.2.1.1")
    pub hostname: String,                  // Resolved hostname from ADS lookup
    pub logger: String,                    // Logger name from PLC
    pub app_name: String,                  // TwinCAT app name
    pub project_name: String,              // TwinCAT project name
    pub task_name: String,                 // Task name within app

    // Log level and timestamps (3 fields)
    pub level: LogLevel,                   // Trace, Debug, Info, Warn, Error, Fatal
    pub plc_timestamp: DateTime<Utc>,      // PLC's internal clock
    pub clock_timestamp: DateTime<Utc>,    // When received by receiver

    // Task execution context (3 fields)
    pub task_index: i32,                   // Task identifier
    pub task_cycle_counter: u32,           // Cycle number for this task
    pub online_change_count: u32,          // Online change count at time of log

    // Message content (1 field)
    pub message: String,                   // Template string with {0}, {name} placeholders

    // Message arguments and context (2 fields)
    pub arguments: BTreeMap<usize, AdsObject>,  // 1-indexed positional arguments
    pub context: BTreeMap<String, AdsObject>,   // Named context data

    // Computed fields (accessed via methods, not stored)
    // - formatted_message: String (computed on demand, cached)
    // - argument_values: Vec<AdsObject> (computed on demand, cached)
}

impl LogEntry {
    /// Format the message template with arguments
    pub fn formatted_message(&self) -> String { /* ... */ }

    /// Get arguments as ordered vec, filling gaps with null
    pub fn argument_values(&self) -> Vec<AdsObject> { /* ... */ }
}
```

### 3.2 LogLevel Enum

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u16)]
pub enum LogLevel {
    Trace = 0,
    Debug = 1,
    Info = 2,
    Warn = 3,
    Error = 4,
    Fatal = 5,
}

impl LogLevel {
    /// Convert to OpenTelemetry severity
    pub fn to_otel_severity(&self) -> opentelemetry_sdk::logs::Severity {
        match self {
            LogLevel::Trace => Severity::TRACE,
            LogLevel::Debug => Severity::DEBUG,
            LogLevel::Info => Severity::INFO,
            LogLevel::Warn => Severity::WARN,
            LogLevel::Error => Severity::ERROR,
            LogLevel::Fatal => Severity::FATAL,
        }
    }
}
```

### 3.3 AdsObject Enum

Represents all TwinCAT scalar and complex types supported by the ADS binary protocol:

```rust
#[derive(Debug, Clone, PartialEq)]
pub enum AdsObject {
    Null,
    Byte(u8),
    Word(u16),
    DWord(u32),
    Real(f32),
    LReal(f64),
    SInt(i8),
    Int(i16),
    DInt(i32),
    USInt(u8),
    UInt(u16),
    UDInt(u32),
    String(String),
    Bool(bool),
    ULarge(u64),
    Large(i64),
    Time(Duration),           // Milliseconds as Duration
    LTime(Duration),          // Nanoseconds as Duration
    Date(DateTime<Utc>),      // Unix timestamp
    DateTime(DateTime<Utc>),  // Unix timestamp
    TimeOfDay(Duration),      // Milliseconds as Duration
    Enum(i64),                // Enum as integer value
    WString(String),          // UTF-16 as String (already decoded)
}

impl AdsObject {
    /// Convert to a displayable string
    pub fn to_string(&self) -> String { /* ... */ }

    /// Attempt conversion to specific type
    pub fn as_string(&self) -> Option<&str> { /* ... */ }
    pub fn as_i64(&self) -> Option<i64> { /* ... */ }
    pub fn as_f64(&self) -> Option<f64> { /* ... */ }
}
```

### 3.4 Message Template Parser

The `MessageFormatter` handles template strings with both positional and named placeholders:

```rust
/// Parses and formats message templates with {0}, {name}, and escaping support
pub struct MessageFormatter {
    template: String,
    tokens: Vec<MessageToken>,
    argument_names: Vec<String>,
    positional_indices: Option<Vec<usize>>,
}

#[derive(Debug)]
enum MessageToken {
    Text(String),
    Placeholder {
        label: String,
        index: Option<usize>,
        alignment: i32,
        format: String,
    },
}

impl MessageFormatter {
    pub fn new(template: &str) -> Result<Self, MessageParseError> { /* ... */ }

    /// Format with named/positional arguments
    pub fn format(&self, args: &[AdsObject]) -> String { /* ... */ }

    /// Get list of argument placeholders in order
    pub fn argument_names(&self) -> &[String] { /* ... */ }

    /// True if all placeholders are positional (e.g., {0}, {1})
    pub fn is_positional(&self) -> bool { /* ... */ }
}
```

### 3.5 Context Data

Context data is stored as key-value pairs in the LogEntry:

```rust
/// Scope context markers from ADS protocol (byte value in binary format)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContextScope {
    Request = 0x00,
    Response = 0x01,
    // Other scopes may be defined in future versions
}
```

## 4. ADS Receiver Module

### 4.1 Architecture

The ADS receiver listens on TCP port 16150 and implements the Beckhoff AMS/ADS protocol. It accepts serialized `LogEntry` objects as binary payloads and hands them off to the dispatcher.

**Recommended approach:** Use raw TCP with manual ADS framing rather than a third-party ADS library. This provides:
- Minimal external dependencies
- Direct control over buffer management for zero-copy parsing
- Explicit error handling for network conditions
- Easy testing with custom protocol frames

**Reference implementations:**
- TwinCAT.Ads (native C# library) — protocol version 1.0
- Protocol specification: Automation Device Specification (ADS)

### 4.2 ADS Receiver Components

```rust
/// Main ADS receiver listening on port 16150
pub struct AdsReceiver {
    listener: TcpListener,
    local_addr: SocketAddr,
    shutdown: Arc<AtomicBool>,
    logger: Logger,
}

impl AdsReceiver {
    /// Create a new receiver bound to 0.0.0.0:16150
    pub async fn new(port: u16, logger: Logger) -> Result<Self, AdsError> { /* ... */ }

    /// Start accepting connections and processing log messages
    pub async fn start(&self, dispatcher: Arc<dyn LogDispatcher>) -> Result<(), AdsError> {
        // Accept connections in a loop
        // For each connection: spawn a handler task
        // Handlers parse binary protocol frames and hand off LogEntry objects
    }

    /// Signal graceful shutdown
    pub fn shutdown(&self) { /* ... */ }
}

/// Connection-level state machine
struct AdsConnection {
    peer_addr: SocketAddr,
    reader: BufReader<ReadHalf<TcpStream>>,
    writer: BufWriter<WriteHalf<TcpStream>>,
    frame_buffer: BytesMut,
    ads_header: Option<AdsHeader>,
}

impl AdsConnection {
    /// Read next AMS/ADS frame from network
    async fn read_frame(&mut self) -> Result<AmsFrame, AdsError> { /* ... */ }

    /// Parse a single LogEntry from the WriteRequest payload
    fn parse_log_entry(&self, payload: &[u8]) -> Result<LogEntry, ParseError> { /* ... */ }
}
```

### 4.3 Binary Protocol Parsing

The receiver parses ADS WriteRequests containing log data:

**AMS Frame Structure:**
```
┌─ AMS Header (32 bytes) ─────────────┐
├─ Target AMS Net ID (6 bytes)        │
├─ Target Port (2 bytes)              │
├─ Source AMS Net ID (6 bytes)        │
├─ Source Port (2 bytes)              │
├─ Command ID (2 bytes): 0x0003       │ (WriteRequest)
├─ State Flags (2 bytes)              │
├─ Data Length (4 bytes)              │
├─ Error Code (4 bytes)               │
└─ Invoke ID (4 bytes)                │
├─ Index Group (4 bytes): 0xF110      │ (LogWrite index)
├─ Index Offset (4 bytes): 0x00000000 │
├─ Write Length (4 bytes): N          │
├─ Write Data (N bytes)               │
└──────────────────────────────────────┘
```

**Log Entry Binary Format (Protocol v1):**
```
┌─ Version (1 byte): 0x01
├─ Message (PLC String)
├─ Logger (PLC String)
├─ Level (u16, little-endian)
├─ PLC Timestamp (i64 Windows FILETIME)
├─ Clock Timestamp (i64 Windows FILETIME)
├─ Task Index (i32, little-endian)
├─ Task Name (PLC String)
├─ Task Cycle Counter (u32, little-endian)
├─ App Name (PLC String)
├─ Project Name (PLC String)
├─ Online Change Count (u32, little-endian)
├─ Arguments and Context (variable length)
│  ├─ Type Byte (1 byte):
│  │  ├─ 0x01: Argument (followed by index byte + object)
│  │  ├─ 0x02: Context (followed by scope byte + name + object)
│  │  └─ 0xFF: End of data
│  └─ Repeats until 0xFF
└─ End Marker (1 byte): 0xFF
```

**PLC String format (null-terminated, 1252 encoding):**
```
┌─ Length (1 byte): N
└─ Data (N bytes): ASCII/1252-encoded string
```

**Object format:**
```
┌─ Type ID (i16, little-endian)
└─ Value (variable, depends on type):
   ├─ 0: null (no data)
   ├─ 1-15: Integer/float/bool types
   ├─ 12: String (PLC String format)
   ├─ 20000-20006: Custom TwinCAT types (TIME, LTIME, DATE, etc.)
```

### 4.4 Connection Handling

```rust
impl AdsReceiver {
    async fn handle_connection(
        peer_addr: SocketAddr,
        socket: TcpStream,
        dispatcher: Arc<dyn LogDispatcher>,
        logger: Logger,
    ) {
        let mut conn = AdsConnection::new(socket, peer_addr);
        
        loop {
            match conn.read_frame().await {
                Ok(frame) => {
                    if frame.command_id == 0x0003 { // WriteRequest
                        match conn.parse_log_entry(&frame.data) {
                            Ok(entry) => {
                                let _ = dispatcher.dispatch(vec![entry]).await;
                            }
                            Err(e) => {
                                logger.warn("failed to parse log entry", ?e);
                                // Send error response but stay connected
                            }
                        }
                    }
                    // Acknowledge the request
                    conn.send_response(0x0000).await.ok(); // No error
                }
                Err(AdsError::ConnectionClosed) => break,
                Err(e) => {
                    logger.warn("frame read error", ?e);
                    break;
                }
            }
        }
    }
}
```

## 5. Log Dispatcher

The dispatcher is the central async hub that collects log entries from the ADS receiver and fans them out to OTEL exporters with backpressure handling.

### 5.1 Dispatcher Trait and Implementation

```rust
/// Core dispatcher trait for async log dispatch
#[async_trait]
pub trait LogDispatcher: Send + Sync {
    /// Send logs for processing (may block on backpressure)
    async fn dispatch(&self, entries: Vec<LogEntry>) -> Result<(), DispatchError>;

    /// Flush pending batches and wait for all exporters to finish
    async fn flush(&self) -> Result<(), DispatchError>;

    /// Graceful shutdown: flush, wait for inflight, then stop accepting new logs
    async fn shutdown(&self) -> Result<(), DispatchError>;
}

/// Default implementation using tokio channels and exporters
pub struct TokioDispatcher {
    // Incoming log channel with bounded capacity
    tx: mpsc::Sender<Vec<LogEntry>>,
    rx: Arc<Mutex<mpsc::Receiver<Vec<LogEntry>>>>,

    // Exporter instances
    exporters: Vec<Arc<dyn OtelExporter>>,

    // Metrics and state
    metrics: Arc<DispatcherMetrics>,
    shutdown: Arc<AtomicBool>,
}

#[derive(Debug)]
pub struct DispatcherMetrics {
    pub logs_received: Arc<AtomicU64>,
    pub logs_exported: Arc<AtomicU64>,
    pub logs_dropped: Arc<AtomicU64>,
    pub export_errors: Arc<AtomicU64>,
}
```

### 5.2 Implementation Details

```rust
impl TokioDispatcher {
    /// Create a new dispatcher with specified exporters
    pub async fn new(
        exporters: Vec<Arc<dyn OtelExporter>>,
        buffer_size: usize,
        logger: Logger,
    ) -> Result<Self, DispatchError> {
        let (tx, rx) = mpsc::channel(buffer_size);
        
        let dispatcher = Self {
            tx,
            rx: Arc::new(Mutex::new(rx)),
            exporters,
            metrics: Arc::new(DispatcherMetrics::default()),
            shutdown: Arc::new(AtomicBool::new(false)),
        };

        // Start background worker task
        dispatcher.spawn_worker();

        Ok(dispatcher)
    }

    fn spawn_worker(&self) {
        let rx = Arc::clone(&self.rx);
        let exporters = self.exporters.clone();
        let metrics = Arc::clone(&self.metrics);
        let shutdown = Arc::clone(&self.shutdown);
        let logger = self.logger.clone();

        tokio::spawn(async move {
            let mut rx = rx.lock().await;
            
            loop {
                tokio::select! {
                    Some(batch) = rx.recv() => {
                        metrics.logs_received.fetch_add(batch.len() as u64, Ordering::Relaxed);

                        // Export to all exporters in parallel
                        let export_futures = exporters.iter()
                            .map(|exporter| exporter.export(batch.clone()));

                        let results = futures::future::join_all(export_futures).await;

                        for (idx, result) in results.iter().enumerate() {
                            match result {
                                Ok(count) => {
                                    metrics.logs_exported.fetch_add(*count as u64, Ordering::Relaxed);
                                }
                                Err(e) => {
                                    metrics.export_errors.fetch_add(1, Ordering::Relaxed);
                                    logger.error("exporter failed", idx, ?e);
                                }
                            }
                        }
                    }
                    _ = tokio::time::sleep(Duration::from_secs(1)), if shutdown.load(Ordering::Relaxed) => {
                        break;
                    }
                }
            }

            logger.info("dispatcher worker shutting down");
        });
    }
}

#[async_trait]
impl LogDispatcher for TokioDispatcher {
    async fn dispatch(&self, entries: Vec<LogEntry>) -> Result<(), DispatchError> {
        if self.shutdown.load(Ordering::Relaxed) {
            return Err(DispatchError::Shutdown);
        }

        self.tx.send(entries).await
            .map_err(|_| DispatchError::ChannelClosed)?;

        Ok(())
    }

    async fn flush(&self) -> Result<(), DispatchError> {
        // Wait for all exporters to complete pending batches
        futures::future::join_all(
            self.exporters.iter()
                .map(|e| e.flush())
        ).await;

        Ok(())
    }

    async fn shutdown(&self) -> Result<(), DispatchError> {
        self.shutdown.store(true, Ordering::Relaxed);
        self.flush().await?;
        Ok(())
    }
}
```

### 5.3 Backpressure Handling

The `mpsc::channel` with bounded capacity provides built-in backpressure:
- When the buffer is full, `send()` blocks the caller (the ADS receiver)
- This naturally slows down incoming connections if exporters can't keep up
- Prevents unbounded memory growth in the face of slow exporters
- Each exporter can be configured with different batch sizes and timeouts

## 6. Configuration System

### 6.1 Configuration File Format (TOML)

```toml
[service]
# Windows service name (must match install command)
service_name = "log4tc"

# Service display name in Windows Services UI
display_name = "log4TC Logging Service"

[ads]
# Port to listen for ADS connections
port = 16150

# How many pending connections to queue
listen_backlog = 128

# Timeout for connection read operations (seconds)
read_timeout = 30

[dispatcher]
# Channel buffer capacity (logs queued before backpressure)
buffer_capacity = 1000

# Default batch size for exporters (if not overridden per exporter)
default_batch_size = 100

# Maximum time to wait for a batch before flushing (milliseconds)
batch_timeout_ms = 5000

[[exporters]]
# First exporter: gRPC OTLP
type = "grpc"
name = "otel-grpc"
endpoint = "http://localhost:4317"
timeout_seconds = 30
batch_size = 100

# Enable TLS/mTLS (optional)
[exporters.tls]
enabled = false
cert_path = "/path/to/cert.pem"
key_path = "/path/to/key.pem"
ca_cert_path = "/path/to/ca.pem"

[[exporters]]
# Second exporter: HTTP OTLP
type = "http"
name = "otel-http"
endpoint = "https://otlp.example.com:4318"
timeout_seconds = 30
batch_size = 50

# Optional HTTP headers
[exporters.headers]
"Authorization" = "Bearer token123"
"X-Custom-Header" = "value"

[logging]
# Tracing/logging configuration for the service itself
level = "info"
# Log format: "compact", "pretty", "json"
format = "json"

# Log file path (Windows: %ProgramData%\log4tc\service.log)
file = "/var/log/log4tc.log"

# Rolling file configuration
max_size_mb = 10
max_backups = 5

[filters]
# Global log filtering (before sending to exporters)
# Logs matching these criteria are included

[[filters.include]]
type = "level"
# Include logs at these levels: trace, debug, info, warn, error, fatal
min_level = "debug"

[[filters.include]]
type = "logger"
# Include logs from these loggers (regex or exact match)
pattern = "^Application\\..*"
mode = "regex"

[[filters.exclude]]
type = "message"
# Exclude logs with messages matching this pattern
pattern = ".*diagnostic.*"
mode = "regex"
```

### 6.2 Configuration Loading and Validation

```rust
use serde::{Deserialize, Serialize};
use config::{Config, ConfigError, File, FileFormat};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ServiceConfig {
    pub service: ServiceSettings,
    pub ads: AdsSettings,
    pub dispatcher: DispatcherSettings,
    pub exporters: Vec<ExporterConfig>,
    pub logging: LoggingConfig,
    pub filters: Option<FilterConfig>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ServiceSettings {
    pub service_name: String,
    pub display_name: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AdsSettings {
    pub port: u16,
    pub listen_backlog: u32,
    pub read_timeout: u64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DispatcherSettings {
    pub buffer_capacity: usize,
    pub default_batch_size: usize,
    pub batch_timeout_ms: u64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ExporterConfig {
    #[serde(rename = "type")]
    pub exporter_type: String,  // "grpc" or "http"
    pub name: String,
    pub endpoint: String,
    pub timeout_seconds: u64,
    pub batch_size: Option<usize>,
    pub tls: Option<TlsConfig>,
    pub headers: Option<std::collections::BTreeMap<String, String>>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TlsConfig {
    pub enabled: bool,
    pub cert_path: Option<String>,
    pub key_path: Option<String>,
    pub ca_cert_path: Option<String>,
}

impl ServiceConfig {
    /// Load configuration from TOML file
    pub async fn from_file(path: &Path) -> Result<Self, ConfigError> {
        let config = Config::builder()
            .add_source(File::from(path).format(FileFormat::Toml))
            .build()?;

        let service_config: ServiceConfig = config.try_deserialize()?;
        service_config.validate()?;
        Ok(service_config)
    }

    /// Validate configuration (e.g., valid ports, endpoints reachable, etc.)
    pub fn validate(&self) -> Result<(), ConfigError> {
        // Check port range
        if self.ads.port == 0 {
            return Err(ConfigError::Message("ADS port cannot be 0".to_string()));
        }

        // Check at least one exporter is configured
        if self.exporters.is_empty() {
            return Err(ConfigError::Message("At least one exporter must be configured".to_string()));
        }

        // Validate each exporter
        for exporter in &self.exporters {
            exporter.validate()?;
        }

        Ok(())
    }
}

impl ExporterConfig {
    fn validate(&self) -> Result<(), ConfigError> {
        match self.exporter_type.as_str() {
            "grpc" | "http" => {}
            _ => return Err(ConfigError::Message(
                format!("Unknown exporter type: {}", self.exporter_type)
            )),
        }

        // Validate endpoint is a valid URL
        url::Url::parse(&self.endpoint)
            .map_err(|e| ConfigError::Message(format!("Invalid endpoint URL: {}", e)))?;

        Ok(())
    }
}
```

### 6.3 Hot Reload with notify Crate

The configuration can be reloaded when the file changes without restarting the service:

```rust
use notify::{Watcher, RecursiveMode, watcher};
use std::sync::mpsc::channel;
use std::time::Duration;

pub struct ConfigReloader {
    config_path: PathBuf,
    tx: Sender<ConfigReloadEvent>,
}

#[derive(Debug)]
pub enum ConfigReloadEvent {
    ConfigUpdated(ServiceConfig),
    Error(String),
}

impl ConfigReloader {
    pub async fn watch(&self) -> Result<(), Box<dyn std::error::Error>> {
        let (tx, rx) = channel();

        let mut watcher = watcher(tx, Duration::from_secs(2))?;
        watcher.watch(&self.config_path, RecursiveMode::NonRecursive)?;

        for event in rx {
            if let Ok(notify::DebouncedEvent::Write(_)) = event {
                match ServiceConfig::from_file(&self.config_path).await {
                    Ok(config) => {
                        let _ = self.tx.send(ConfigReloadEvent::ConfigUpdated(config));
                    }
                    Err(e) => {
                        let _ = self.tx.send(ConfigReloadEvent::Error(e.to_string()));
                    }
                }
            }
        }

        Ok(())
    }
}
```

## 7. Error Handling Strategy

### 7.1 Error Types Hierarchy

Use `thiserror` for library crates and `anyhow` for the application crate:

```rust
// log4tc-core/src/error.rs
use thiserror::Error;

#[derive(Error, Debug)]
pub enum CoreError {
    #[error("invalid log level: {0}")]
    InvalidLogLevel(u16),

    #[error("message template parse error at position {pos}: {reason}")]
    TemplateParseError { pos: usize, reason: String },

    #[error("invalid message argument: {0}")]
    InvalidArgument(String),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

// log4tc-ads/src/error.rs
#[derive(Error, Debug)]
pub enum AdsError {
    #[error("connection refused: {0}")]
    ConnectionRefused(String),

    #[error("invalid frame: {0}")]
    InvalidFrame(String),

    #[error("parse error: {0}")]
    ParseError(#[from] CoreError),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("connection closed")]
    ConnectionClosed,
}

// log4tc-otel/src/error.rs
#[derive(Error, Debug)]
pub enum OtelError {
    #[error("exporter not available: {0}")]
    ExporterNotAvailable(String),

    #[error("export failed: {0}")]
    ExportFailed(String),

    #[error("invalid endpoint: {0}")]
    InvalidEndpoint(String),

    #[error("request timeout")]
    Timeout,

    #[error("http error: {0}")]
    Http(#[from] reqwest::Error),
}

// log4tc-service/src/error.rs
use anyhow::{anyhow, Result};

#[derive(Error, Debug)]
pub enum ServiceError {
    #[error("configuration error: {0}")]
    Config(String),

    #[error("dispatcher error: {0}")]
    Dispatcher(String),

    #[error("ads receiver error: {0}")]
    AdsReceiver(#[from] AdsError),

    #[error("otel exporter error: {0}")]
    Otel(#[from] OtelError),

    #[error("windows service error: {0}")]
    WindowsService(String),
}
```

### 7.2 Structured Logging with tracing Crate

```rust
use tracing::{debug, info, warn, error, instrument, Span};
use tracing_subscriber::fmt::{self, format::FmtSpan};

fn init_logging(config: &LoggingConfig) -> Result<()> {
    let fmt_layer = fmt::layer()
        .with_writer(std::io::stderr)
        .with_target(true)
        .with_thread_ids(true)
        .with_span_events(FmtSpan::CLOSE);

    tracing_subscriber::registry()
        .with(fmt_layer)
        .init();

    info!("logging initialized");
    Ok(())
}

#[instrument(skip(dispatcher))]
async fn handle_log_entry(entry: LogEntry, dispatcher: Arc<dyn LogDispatcher>) {
    debug!(?entry, "processing log entry");

    match dispatcher.dispatch(vec![entry.clone()]).await {
        Ok(_) => {
            debug!(logger = %entry.logger, "log dispatched");
        }
        Err(e) => {
            error!(?e, "dispatch failed");
        }
    }
}
```

## 8. Windows Service Integration

### 8.1 Service Lifecycle with windows-service Crate

```rust
use windows_service::service::{ServiceControl, ServiceControlAccept, ServiceEventHandler, ServiceHandlerAction, ServiceState, ServiceStatus, ServiceType};
use windows_service::service_dispatcher;

#[derive(Default)]
pub struct ServiceEventHandler {
    shutdown_event: Arc<Event>,
}

impl ServiceEventHandler for ServiceEventHandler {
    fn handle_control(&mut self, control: ServiceControl) -> ServiceHandlerAction {
        match control {
            ServiceControl::Interrogate => ServiceHandlerAction::ReportStatusSuccess,
            ServiceControl::Stop | ServiceControl::Shutdown => {
                self.shutdown_event.set().ok();
                ServiceHandlerAction::ReportStatusSuccess
            }
            ServiceControl::Pause => ServiceHandlerAction::ReportStatusSuccess,
            ServiceControl::Continue => ServiceHandlerAction::ReportStatusSuccess,
            _ => ServiceHandlerAction::ReportStatusSuccess,
        }
    }

    fn set_control_handler(
        &mut self,
        control: ServiceControl,
    ) -> ServiceHandlerAction {
        self.handle_control(control)
    }
}

pub async fn run_service(config: ServiceConfig) -> Result<()> {
    let shutdown = Arc::new(Event::new(true, false)?);

    service_dispatcher::run(
        "log4tc",
        ServiceEventHandler {
            shutdown_event: shutdown.clone(),
        },
        |_args| {
            // Main service loop
            let rt = tokio::runtime::Runtime::new()?;
            rt.block_on(async {
                // Initialize receiver, dispatcher, exporters
                let receiver = AdsReceiver::new(config.ads.port, logger).await?;
                let dispatcher = TokioDispatcher::new(...).await?;

                // Start receiver
                tokio::spawn({
                    let receiver = receiver.clone();
                    async move {
                        receiver.start(dispatcher).await.ok();
                    }
                });

                // Wait for shutdown signal
                shutdown.wait().ok();

                // Graceful shutdown
                dispatcher.shutdown().await?;
                receiver.shutdown();

                Ok(())
            })
        },
    )?;

    Ok(())
}

pub async fn install_service() -> Result<()> {
    let manager = ServiceManager::local_computer()?;
    let exe_path = std::env::current_exe()?;

    let service_info = ServiceInfo {
        name: OsString::from("log4tc"),
        display_name: OsString::from("log4TC Logging Service"),
        service_type: ServiceType::OwnProcess,
        start_type: ServiceStartType::AutoStart,
        error_recovery: Some(ServiceErrorControl::Normal),
        executable_path: exe_path,
        launch_arguments: vec![],
        dependencies: vec![],
        account_name: None,
        account_password: None,
    };

    manager.create_service(&service_info, ServiceStartType::AutoStart)?;
    Ok(())
}

pub async fn uninstall_service() -> Result<()> {
    let manager = ServiceManager::local_computer()?;
    manager.delete_service("log4tc")?;
    Ok(())
}

pub async fn start_service() -> Result<()> {
    let manager = ServiceManager::local_computer()?;
    manager.connect()?.start_service("log4tc")?;
    Ok(())
}

pub async fn stop_service() -> Result<()> {
    let manager = ServiceManager::local_computer()?;
    manager.connect()?.stop_service("log4tc")?;
    Ok(())
}
```

### 8.2 Service Installation and Management

The binary supports command-line arguments for service management:

```bash
# Install the service
log4tc-service.exe --install

# Start the service
log4tc-service.exe --start

# Stop the service
log4tc-service.exe --stop

# Uninstall the service
log4tc-service.exe --uninstall

# Run in console mode (for debugging)
log4tc-service.exe --console
```

### 8.3 Event Log Integration

Logs are written to the Windows Event Log under "Application":

```rust
use winlog::EventLog;

fn init_event_log() -> Result<EventLog> {
    let event_log = EventLog::new("log4tc", "Application")?;
    Ok(event_log)
}

async fn log_to_event_log(event_log: &EventLog, level: i32, message: &str) {
    event_log.log_event(
        message,
        vec![],
        level, // EVENTLOG_INFORMATION_TYPE, EVENTLOG_WARNING_TYPE, EVENTLOG_ERROR_TYPE
    ).ok();
}
```

## 9. Key Trait Definitions

### 9.1 ADS Receiver Trait

```rust
#[async_trait]
pub trait AdsReceiver: Send + Sync {
    /// Listen for incoming ADS connections on the configured port
    async fn listen(&self) -> Result<(), AdsError>;

    /// Handle a single incoming connection
    async fn handle_connection(
        &self,
        peer_addr: SocketAddr,
        socket: TcpStream,
    ) -> Result<(), AdsError>;

    /// Parse a single LogEntry from binary payload
    fn parse_log_entry(&self, payload: &[u8]) -> Result<LogEntry, ParseError>;

    /// Signal graceful shutdown of the receiver
    fn shutdown(&self);

    /// Check if receiver is still running
    fn is_running(&self) -> bool;
}
```

### 9.2 Log Dispatcher Trait

```rust
#[async_trait]
pub trait LogDispatcher: Send + Sync {
    /// Queue log entries for dispatch to exporters
    async fn dispatch(&self, entries: Vec<LogEntry>) -> Result<(), DispatchError>;

    /// Flush all pending batches and wait for completion
    async fn flush(&self) -> Result<(), DispatchError>;

    /// Graceful shutdown: flush, stop accepting new entries, clean up
    async fn shutdown(&self) -> Result<(), DispatchError>;

    /// Get current dispatcher metrics
    fn metrics(&self) -> DispatcherMetrics;
}
```

### 9.3 OTEL Exporter Trait

```rust
#[async_trait]
pub trait OtelExporter: Send + Sync {
    /// Export a batch of log entries to the OTEL backend
    async fn export(&self, entries: Vec<LogEntry>) -> Result<usize, OtelError>;

    /// Flush pending batches (for batching exporters)
    async fn flush(&self) -> Result<(), OtelError>;

    /// Graceful shutdown
    async fn shutdown(&self) -> Result<(), OtelError>;

    /// Get exporter name
    fn name(&self) -> &str;

    /// Health check (can be used for liveness probes)
    async fn health_check(&self) -> Result<(), OtelError>;
}
```

### 9.4 Log Filter Trait

```rust
pub trait LogFilter: Send + Sync {
    /// Return true if the log entry should be included
    fn matches(&self, entry: &LogEntry) -> bool;
}

/// Some built-in filters
pub struct LevelFilter {
    min_level: LogLevel,
}

pub struct LoggerPatternFilter {
    pattern: Regex,
}

pub struct CompositeFilter {
    include: Vec<Box<dyn LogFilter>>,
    exclude: Vec<Box<dyn LogFilter>>,
}
```

## 10. Dependency Table

All Rust crates with version constraints and justification:

| Crate | Version | Crates | Purpose |
|-------|---------|--------|---------|
| tokio | ^1.35 | log4tc-ads, log4tc-service | Async runtime, TCP listener, channels |
| bytes | ^1.5 | log4tc-ads | Zero-copy byte buffer handling |
| serde | ^1.0 + derive | log4tc-core, log4tc-service | Serialization framework |
| toml | ^0.8 | log4tc-service | TOML config parsing |
| config | ^0.13 | log4tc-service | Hierarchical config loading with validation |
| notify | ^6.1 | log4tc-service | File watcher for config hot-reload |
| chrono | ^0.4 | log4tc-core | DateTime and timestamp handling |
| uuid | ^1.6 | log4tc-otel | Trace/span IDs in OTEL |
| opentelemetry | ^0.21 | log4tc-otel | OTEL SDK core |
| opentelemetry-proto | ^0.21 | log4tc-otel | OTLP protocol buffers |
| tonic | ^0.11 | log4tc-otel | gRPC client for OTLP |
| reqwest | ^0.11 | log4tc-otel | HTTP client for OTLP over HTTP |
| futures | ^0.3 | log4tc-service | Async utilities (join_all, select!) |
| thiserror | ^1.0 | log4tc-ads, log4tc-core, log4tc-otel | Ergonomic error definitions |
| anyhow | ^1.0 | log4tc-service | Flexible error handling for app |
| tracing | ^0.1 | All crates | Structured logging framework |
| tracing-subscriber | ^0.3 | log4tc-service | Logging initialization and formatting |
| windows-service | ^0.6 | log4tc-service | Windows service integration |
| windows | ^0.52 | log4tc-service | Windows API bindings (Event Log, etc.) |
| log | ^0.4 | All crates | Standard logging facade |
| url | ^2.5 | log4tc-otel, log4tc-service | URL parsing and validation |
| regex | ^1.10 | log4tc-core | Message filtering patterns |
| prost | ^0.12 | log4tc-otel | Protobuf code generation |
| async-trait | ^0.1 | All crates | Async trait support |

**Development dependencies:**
| Crate | Version | Purpose |
|-------|---------|---------|
| tokio-test | ^0.4 | Testing async code |
| mockall | ^0.12 | Mock trait implementations for tests |
| proptest | ^1.4 | Property-based testing for parsers |

## 11. Performance Considerations

### 11.1 Zero-Copy Parsing

The ADS binary parser uses `bytes::BytesMut` and `std::io::Cursor` to avoid copying during parsing:

```rust
pub fn parse_log_entry_zerocopy(data: &[u8]) -> Result<LogEntry> {
    let mut cursor = Cursor::new(data);
    
    // Read version (1 byte) — no copy, just position increment
    let version = cursor.read_u8()?;
    if version != 1 {
        return Err(ParseError::UnsupportedVersion(version));
    }

    // Read strings without copying — return &str backed by the input slice
    let message = read_string_ref(&mut cursor)?;  // Returns &str into data
    let logger = read_string_ref(&mut cursor)?;

    // ...
    
    Ok(LogEntry {
        message: message.to_owned(),  // Only copy when creating owned String
        logger: logger.to_owned(),
        // ...
    })
}

fn read_string_ref<'a>(cursor: &mut Cursor<&'a [u8]>) -> Result<&'a str> {
    let pos = cursor.position() as usize;
    let len = cursor.read_u8()? as usize;
    let bytes = &cursor.get_ref()[pos + 1..pos + 1 + len];
    Ok(std::str::from_utf8(bytes)?)
}
```

### 11.2 Arena Allocation

For high-throughput scenarios, use arena allocation for temporary objects:

```rust
use bumpalo::Bump;

pub fn parse_batch_arena(data: &[u8], arena: &Bump) -> Result<Vec<LogEntry>> {
    let mut entries = Vec::new_in(arena);
    // All temporary allocations use the arena
    // Single deallocation when arena is dropped
}
```

### 11.3 Batch Processing

The dispatcher batches logs before exporting:

```rust
pub struct BatchingExporter {
    batch_size: usize,
    batch_timeout: Duration,
    pending_batch: Arc<Mutex<Vec<LogEntry>>>,
}

#[async_trait]
impl OtelExporter for BatchingExporter {
    async fn export(&self, entries: Vec<LogEntry>) -> Result<usize> {
        let mut batch = self.pending_batch.lock().await;
        batch.extend(entries.clone());

        let count = if batch.len() >= self.batch_size {
            self.flush_batch(&batch).await?
        } else {
            0
        };

        Ok(count)
    }

    async fn flush(&self) -> Result<()> {
        let batch = self.pending_batch.lock().await.clone();
        if !batch.is_empty() {
            self.flush_batch(&batch).await?;
        }
        Ok(())
    }
}
```

### 11.4 Memory Efficiency

- Use `Arc<[T]>` instead of `Vec<T>` for shared read-only data
- Use `SmallVec` for vectors with typical size < 10 elements (e.g., arguments)
- Implement `Cow<str>` where string ownership varies
- Profile with `perf` on Linux, `cargo flamegraph` for visual profiling

## 12. Security Considerations

### 12.1 ADS Network Exposure

The ADS receiver listens on port 16150, which should **not** be exposed to untrusted networks:

```rust
// Bind to localhost by default for local TwinCAT connections
let listener = TcpListener::bind("127.0.0.1:16150").await?;

// Or use explicit allow-list of source IPs:
#[derive(Debug, Clone)]
pub struct AdsReceiverConfig {
    pub allowed_sources: Vec<IpAddr>,  // E.g., ["192.168.1.0/24", "127.0.0.1"]
}

async fn validate_connection(peer_addr: &SocketAddr, config: &AdsReceiverConfig) -> bool {
    config.allowed_sources.iter().any(|ip| {
        // Match against CIDR or exact IP
        peer_addr.ip() == ip
    })
}
```

### 12.2 TLS for OTLP Export

The exporter supports TLS/mTLS for secure communication with the OTEL collector:

```rust
pub struct GrpcOtelExporter {
    endpoint: String,
    client: Option<tonic::transport::Channel>,
    tls_config: Option<TlsConfig>,
}

impl GrpcOtelExporter {
    pub async fn new(config: &ExporterConfig) -> Result<Self> {
        let mut channel_builder = tonic::transport::Channel::from_shared(
            config.endpoint.clone()
        )?;

        // Add TLS if configured
        if let Some(tls) = &config.tls {
            if tls.enabled {
                let tls_config = tonic::transport::ClientTlsConfig::new()
                    .domain_name("otel.collector.local")
                    .load_certs(tls.ca_cert_path.as_deref())?
                    .load_client_certs(
                        tls.cert_path.as_deref(),
                        tls.key_path.as_deref(),
                    )?;

                channel_builder = channel_builder.tls_config(tls_config)?;
            }
        }

        let channel = channel_builder.connect().await?;

        Ok(Self {
            endpoint: config.endpoint.clone(),
            client: Some(channel),
            tls_config: config.tls.clone(),
        })
    }
}
```

### 12.3 Input Validation

All untrusted input (binary protocol, config files) must be validated:

```rust
// Version check
if version != 1 {
    return Err(ParseError::UnsupportedVersion(version));
}

// String length limits
const MAX_STRING_LEN: usize = 10_000;
if len > MAX_STRING_LEN {
    return Err(ParseError::StringTooLong(len));
}

// Endpoint URL validation
url::Url::parse(&endpoint)
    .map_err(|e| ConfigError::InvalidEndpoint(e.to_string()))?;

// Configuration schema validation with serde
#[derive(Deserialize)]
struct ExporterConfig {
    #[serde(rename = "type")]
    #[validate(length(min = 1, max = 50))]
    exporter_type: String,
}
```

### 12.4 Resource Limits

Prevent denial-of-service attacks via resource exhaustion:

```rust
pub struct DispatcherConfig {
    /// Maximum number of queued log entries
    pub max_queue_size: usize,

    /// Maximum size of a single batch
    pub max_batch_size: usize,

    /// Maximum number of concurrent connections to ADS receiver
    pub max_connections: usize,

    /// Timeout for parsing a single frame
    pub parse_timeout: Duration,
}

impl AdsReceiver {
    async fn handle_connection_limited(
        peer_addr: SocketAddr,
        socket: TcpStream,
    ) -> Result<(), AdsError> {
        // Enforce parse timeout
        tokio::time::timeout(
            Duration::from_secs(30),
            Self::handle_connection_inner(peer_addr, socket),
        ).await?
    }
}
```

---

## Implementation Roadmap

1. **Phase 1**: Core data model (log4tc-core)
   - LogEntry, LogLevel, AdsObject structs
   - Message template parser
   - Basic filtering

2. **Phase 2**: ADS receiver (log4tc-ads)
   - TCP listener on port 16150
   - Binary protocol v1 parser
   - Connection handling

3. **Phase 3**: OTEL exporter (log4tc-otel)
   - gRPC OTLP exporter
   - HTTP OTLP exporter
   - Batch collection and flushing

4. **Phase 4**: Dispatcher (log4tc-service)
   - Async channel-based dispatcher
   - Configuration loading and validation
   - Graceful shutdown

5. **Phase 5**: Windows service (log4tc-service)
   - Service lifecycle management
   - Installation/uninstallation
   - Event log integration

6. **Phase 6**: Testing and production hardening
   - Unit tests for all modules
   - Integration tests
   - Load testing
   - Security audit

---

## Appendix: Example Configuration File

Location: `%ProgramData%\log4tc\config\appsettings.toml`

```toml
[service]
service_name = "log4tc"
display_name = "log4TC PLC Logging Service"

[ads]
port = 16150
listen_backlog = 128
read_timeout = 30

[dispatcher]
buffer_capacity = 5000
default_batch_size = 100
batch_timeout_ms = 5000

[[exporters]]
type = "grpc"
name = "grafana-loki"
endpoint = "http://grafana.internal:4317"
timeout_seconds = 10
batch_size = 100

[[exporters]]
type = "http"
name = "datadog-otel"
endpoint = "https://http-intake.logs.datadoghq.eu:443/v1/input"
timeout_seconds = 30
batch_size = 50

[exporters.headers]
"DD-API-KEY" = "your-api-key-here"

[logging]
level = "info"
format = "json"
file = "C:\\ProgramData\\log4tc\\service.log"
max_size_mb = 10
max_backups = 5

[[filters.include]]
type = "level"
min_level = "debug"

[[filters.exclude]]
type = "message"
pattern = ".*heartbeat.*"
mode = "regex"
```

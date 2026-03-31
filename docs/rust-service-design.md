# Rust Service Design Specification - Log4TC

**Document Version**: 1.0  
**Last Updated**: March 31, 2026

---

## Executive Summary

This document specifies the detailed design of the Log4TC Rust service, which replaces the .NET implementation. The service is an async, tokio-based application that receives log entries via ADS (legacy) and OTEL (primary) protocols, routes them through a dispatcher, and exports to observability collectors.

---

## Crate Architecture

### Top-Level Workspace

```
log4tc/
├── Cargo.toml (workspace)
├── crates/
│   ├── log4tc-core/        # Core types, models, configuration
│   ├── log4tc-ads/         # ADS binary protocol (legacy)
│   ├── log4tc-otel/        # OTEL protocol (primary)
│   ├── log4tc-service/     # Service orchestration
│   └── log4tc-benches/     # Performance benchmarks
```

---

## Module Breakdown

### 1. log4tc-core

**Responsibility**: Core types, shared models, and configuration

**Files**:
```
src/
├── lib.rs           # Module declarations, public API
├── models.rs        # LogLevel, LogEntry, LogRecord
├── formatter.rs     # Message template formatting
├── config.rs        # Configuration structs (TOML/JSON)
└── error.rs         # Error types and propagation
```

**Key Types**:

#### LogLevel Enum
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    Trace = 0,
    Debug = 1,
    Info = 2,
    Warn = 3,
    Error = 4,
    Fatal = 5,
}

impl LogLevel {
    pub fn to_otel_severity_number(&self) -> i32;    // 1, 5, 9, 13, 17, 21
    pub fn to_otel_severity_text(&self) -> &'static str;  // "TRACE", "DEBUG", ...
}
```

#### LogEntry Struct
```rust
pub struct LogEntry {
    pub version: u8,
    pub message: String,
    pub logger: String,
    pub level: LogLevel,
    pub plc_timestamp: DateTime<Utc>,
    pub clock_timestamp: DateTime<Utc>,
    pub task_index: i32,
    pub task_name: String,
    pub task_cycle_counter: u32,
    pub app_name: String,
    pub project_name: String,
    pub hostname: String,
    pub online_change_count: u32,
    pub source: String,          // IP:Port of sender
    pub arguments: HashMap<usize, serde_json::Value>,
    pub context: HashMap<String, serde_json::Value>,
}
```

#### LogRecord Struct (OTEL)
```rust
pub struct LogRecord {
    pub timestamp: DateTime<Utc>,
    pub body: String,
    pub severity_number: i32,
    pub resource_attributes: HashMap<String, String>,
    pub scope_attributes: HashMap<String, String>,
    pub log_attributes: HashMap<String, serde_json::Value>,
}

impl LogRecord {
    pub fn from_log_entry(entry: &LogEntry) -> Self { ... }
}
```

**MessageFormatter**:
```rust
pub struct MessageFormatter;

impl MessageFormatter {
    pub fn format(template: &str, arguments: &HashMap<usize, serde_json::Value>) -> Result<String>;
}

// Supports positional: "Value is {0}, name is {1}"
// Supports named: "Value is {value}, name is {name}"
```

**Configuration** (TOML/JSON):
```rust
pub struct AppSettings {
    pub logging: LoggingConfig,
    pub receiver: ReceiverConfig,
    pub outputs: Vec<OutputConfig>,
    pub service: ServiceConfig,
}

pub struct ReceiverConfig {
    pub host: String,           // Default: 127.0.0.1
    pub ads_port: u16,          // Default: 16150
    pub otel_http_port: u16,    // Default: 4318
    pub otel_grpc_port: u16,    // Default: 4317
}

pub struct ServiceConfig {
    pub channel_capacity: usize,        // Default: 10,000
    pub shutdown_timeout_secs: u64,    // Default: 30
}
```

---

### 2. log4tc-ads

**Responsibility**: ADS binary protocol parsing and TCP server

**Files**:
```
src/
├── lib.rs           # Module declarations
├── protocol.rs      # ADS types, version, constants
├── parser.rs        # Binary protocol parser (with security limits)
├── listener.rs      # TCP server, connection handling
└── error.rs         # ADS error types
```

**Key Types**:

#### AdsProtocolVersion
```rust
pub enum AdsProtocolVersion {
    V1 = 1,
}

pub const ADS_PROTOCOL_VERSION: u8 = 1;
pub const ADS_DEFAULT_PORT: u16 = 16150;
```

#### AdsLogEntry
```rust
pub struct AdsLogEntry {
    pub version: AdsProtocolVersion,
    pub message: String,
    pub logger: String,
    pub level: LogLevel,
    pub plc_timestamp: DateTime<Utc>,
    pub clock_timestamp: DateTime<Utc>,
    pub task_index: i32,
    pub task_name: String,
    pub task_cycle_counter: u32,
    pub app_name: String,
    pub project_name: String,
    pub online_change_count: u32,
    pub arguments: HashMap<usize, serde_json::Value>,
    pub context: HashMap<String, serde_json::Value>,
}
```

#### AdsParser
```rust
pub struct AdsParser;

impl AdsParser {
    pub fn parse(data: &[u8]) -> Result<AdsLogEntry> {
        // Parse binary format with security limits:
        // - Max string length: 65 KB
        // - Max arguments: 32
        // - Max context vars: 64
        // - Max message size: 1 MB
    }
}
```

#### AdsListener
```rust
pub struct AdsListener {
    addr: SocketAddr,
    tx: Sender<LogEntry>,
    max_connections: usize,
}

impl AdsListener {
    pub async fn listen(
        addr: SocketAddr,
        tx: Sender<LogEntry>,
    ) -> Result<()> {
        // 1. Create TCP listener on addr
        // 2. Loop accepting connections
        // 3. For each connection:
        //    - Check semaphore for max connections
        //    - Spawn async task for connection handler
        //    - With timeout (300 sec)
        // 4. Handler:
        //    - Read message from socket
        //    - Parse with AdsParser
        //    - Send ACK or error NAK
        //    - Send LogEntry to channel (tx)
    }
}

struct ConnectionHandler {
    socket: TcpStream,
    timeout: Duration,
    tx: Sender<LogEntry>,
}
```

**Error Handling**:
```rust
pub enum AdsError {
    InvalidVersion(u8),
    ParseError(String),
    IncompleteMessage { expected: usize, got: usize },
    InvalidStringEncoding(String),
    InvalidTimestamp(String),
    IoError(io::Error),
}
```

---

### 3. log4tc-otel

**Responsibility**: OTEL protocol receiver and exporter

**Files**:
```
src/
├── lib.rs           # Module declarations
├── receiver.rs      # HTTP/gRPC server (Axum)
├── exporter.rs      # HTTP client to OTEL collector
├── mapping.rs       # LogEntry ↔ OTEL LogRecord conversion
└── error.rs         # OTEL error types
```

**Key Types**:

#### OtelHttpReceiver
```rust
pub struct OtelHttpReceiver {
    addr: SocketAddr,
    tx: Sender<LogEntry>,
}

impl OtelHttpReceiver {
    pub async fn start(
        addr: SocketAddr,
        tx: Sender<LogEntry>,
    ) -> Result<()> {
        // 1. Create Axum router
        // 2. Add route: POST /v1/logs
        // 3. Bind to addr
        // 4. For each POST /v1/logs:
        //    - Validate OTEL LogsData format
        //    - Extract LogRecord
        //    - Convert to LogEntry
        //    - Send to channel
        //    - Return 200 OK or 400/429
    }
}
```

**Exporter**:
```rust
pub struct OtelExporter {
    client: reqwest::Client,
    endpoint: String,
    batch_size: usize,
    max_retries: usize,
}

impl OtelExporter {
    pub async fn export(&self, records: Vec<LogRecord>) -> Result<()> {
        // 1. Batch records if needed
        // 2. Serialize to OTEL LogsData JSON
        // 3. POST to collector endpoint
        // 4. On failure, retry with exponential backoff:
        //    100ms → 200ms → 400ms → 5s cap
        // 5. Log success/failure
    }
}

pub struct ExportConfig {
    pub endpoint: String,       // https://localhost:4318/v1/logs
    pub batch_size: usize,      // Default: 100
    pub timeout_secs: u64,      // Default: 30
    pub max_retries: usize,     // Default: 3
}
```

**Mapping**:
```rust
pub struct OtelMapping;

impl OtelMapping {
    pub fn log_entry_to_record(entry: &LogEntry) -> LogRecord {
        // Resource attributes:
        //   service.name = project_name
        //   service.instance.id = app_name
        //   host.name = hostname
        //   process.pid = task_index
        //   process.command_line = task_name
        //
        // Scope attributes:
        //   logger.name = logger
        //
        // Log attributes:
        //   plc.timestamp = plc_timestamp (ISO8601)
        //   task.cycle = task_cycle_counter
        //   online.changes = online_change_count
        //   source.address = source (IP:Port)
        //   arg.0 = arguments[0] (for each argument)
        //   context.* = context (for each context var)
    }
}
```

---

### 4. log4tc-service

**Responsibility**: Service orchestration, main entry point

**Files**:
```
src/
├── main.rs          # Entry point, tracing setup
├── service.rs       # Log4TcService orchestrator
├── dispatcher.rs    # LogDispatcher (channel consumer)
└── config.rs        # Config loading from files
```

**Service Lifecycle**:

```rust
pub struct Log4TcService {
    ads_listener: AdsListener,
    otel_receiver: OtelHttpReceiver,
    otel_exporter: OtelExporter,
    dispatcher: LogDispatcher,
}

impl Log4TcService {
    pub async fn run() -> Result<()> {
        // 1. Load configuration (from file or env)
        let config = AppSettings::load()?;
        
        // 2. Initialize tracing/logging
        tracing_subscriber::fmt()
            .with_env_filter(EnvFilter::from_default_env())
            .init();
        
        // 3. Create MPSC channel for log entries
        let (tx, rx) = channel(config.service.channel_capacity);
        
        // 4. Create receivers (ADS + OTEL)
        let ads_listener = AdsListener::new(
            format!("{}:{}", config.receiver.host, config.receiver.ads_port),
            tx.clone()
        )?;
        
        let otel_receiver = OtelHttpReceiver::new(
            format!("{}:{}", config.receiver.host, config.receiver.otel_http_port),
            tx.clone()
        )?;
        
        let otel_exporter = OtelExporter::new(config.otel_export.clone())?;
        
        // 5. Create dispatcher
        let dispatcher = LogDispatcher::new(rx, otel_exporter)?;
        
        // 6. Spawn async tasks
        tokio::select! {
            res = ads_listener.listen() => {
                eprintln!("ADS listener stopped: {:?}", res);
            }
            res = otel_receiver.start() => {
                eprintln!("OTEL receiver stopped: {:?}", res);
            }
            res = dispatcher.dispatch() => {
                eprintln!("Dispatcher stopped: {:?}", res);
            }
            _ = signal::ctrl_c() => {
                println!("Shutdown signal received");
            }
        }
        
        // 7. Graceful shutdown
        self.shutdown(config.service.shutdown_timeout_secs).await?;
        
        Ok(())
    }
    
    async fn shutdown(&mut self, timeout_secs: u64) -> Result<()> {
        // 1. Signal all tasks to stop
        // 2. Wait for in-flight operations (up to timeout_secs)
        // 3. Force shutdown if timeout exceeded
        // 4. Close all connections
    }
}
```

**LogDispatcher**:

```rust
pub struct LogDispatcher {
    rx: Receiver<LogEntry>,
    otel_exporter: OtelExporter,
}

impl LogDispatcher {
    pub async fn dispatch(&mut self) -> Result<()> {
        while let Some(entry) = self.rx.recv().await {
            // 1. Format message (resolve template)
            let formatted_msg = MessageFormatter::format(&entry.message, &entry.arguments)?;
            
            // 2. Convert to OTEL LogRecord
            let mut record = LogRecord::from_log_entry(&entry);
            record.body = formatted_msg;
            
            // 3. Export to OTEL collector
            self.otel_exporter.export(vec![record]).await?;
            
            // 4. Log success/failure
        }
        Ok(())
    }
}
```

---

## Concurrency Model

### Async Runtime (Tokio)

```
Main Thread (tokio runtime)
│
├─→ Task: ads_listener.listen()
│   ├─ Accepts TCP connections on port 16150
│   ├─ Per connection spawns handler task
│   ├─ Handler: read → parse → send LogEntry → ACK/NAK
│   └─ Uses: Arc<Semaphore> for max 100 concurrent connections
│
├─→ Task: otel_receiver.start()
│   ├─ Axum HTTP server on port 4318
│   ├─ Route: POST /v1/logs
│   ├─ Per request: extract LogRecord → convert to LogEntry
│   └─ Send LogEntry to channel
│
├─→ Task: dispatcher.dispatch()
│   ├─ Consumer of MPSC channel
│   ├─ Per LogEntry:
│   │  ├─ Format message template
│   │  ├─ Convert to OTEL LogRecord
│   │  └─ Export (may batch)
│   └─ Backpressure: If channel full, senders block
│
└─→ Signal handler (Ctrl-C)
    └─ Initiates graceful shutdown
```

### Channel-Based Flow Control

```
ADS Listener ┐
             ├─→ [MPSC Channel (capacity: 10,000)] ─→ Dispatcher
OTEL HTTP ──┘

When channel is full:
- ADS: Returns error NAK to client
- OTEL: Returns 429 Too Many Requests
- Sender blocks until space available

Backpressure cascade:
Slow exporter → Full channel → ADS/OTEL throttle → PLC backs off
```

---

## Configuration

### File Format (TOML)

```toml
[logging]
level = "info"
format = "json"

[receiver]
host = "127.0.0.1"
ads_port = 16150
otel_http_port = 4318
otel_grpc_port = 4317

[[outputs]]
type = "otel"
endpoint = "https://collector.example.com:4318/v1/logs"
batch_size = 100
max_retries = 3

[service]
channel_capacity = 10000
shutdown_timeout_secs = 30
```

### Environment Variables

```bash
# Override config file location
export LOG4TC_CONFIG=/etc/log4tc/config.toml

# OTEL exporter configuration
export OTEL_EXPORTER_OTLP_ENDPOINT=https://collector:4318
export OTEL_EXPORTER_OTLP_HEADERS=Authorization=Bearer%20token123

# Rust logging (if no log level in config)
export RUST_LOG=info,log4tc=debug
```

---

## Error Handling Strategy

### By Component

**ADS Listener**:
- TCP connection error → Log error, continue listening
- Parse error → Send NAK, continue
- Channel full → Return backpressure NAK
- Timeout on slow client → Close connection

**OTEL Receiver**:
- HTTP parsing error → Return 400 Bad Request
- Invalid OTEL format → Return 400
- Channel full → Return 429 Too Many Requests
- Serialization error → Return 500 Internal Server Error

**Dispatcher**:
- Message formatting error → Log warning, skip entry
- Export failure → Retry with backoff, eventually drop
- Channel empty → Wait (no backpressure)

**Service**:
- Config file not found → Fail fast with error
- Port binding fails → Fail fast with error
- Task panic → Log, continue other tasks

### Retry Strategy (OTEL Export)

```
Attempt 1: Wait 100ms, retry
Attempt 2: Wait 200ms, retry
Attempt 3: Wait 400ms, retry
Attempt 4: Wait 5s, retry (capped)
Attempt 5+: Drop entry, log failure
```

---

## Security Considerations

### Current Limitations

- No authentication on ADS receiver (port 16150)
- No TLS on OTEL HTTP (unless configured)
- No PII detection or scrubbing
- No rate limiting per source

### Hardening (Implemented)

- **Input validation**: All string lengths, argument counts checked
- **Timeout**: 300-second connection timeout (Slowloris mitigation)
- **Connection limits**: Max 100 concurrent connections
- **Message size limits**: 1 MB max total, 65 KB per string
- **Type validation**: All type tags validated before parsing

### Future Enhancements

- TLS/HTTPS enforcement for OTEL exporter
- API key / Bearer token authentication
- Rate limiting per IP/source
- PII detection and masking
- Audit logging of configuration changes

---

## Testing Strategy

### Unit Tests

- LogLevel conversions
- Message formatting (templates, placeholders)
- FILETIME timestamp conversions
- Configuration parsing
- Error handling

### Integration Tests

- ADS protocol parsing (valid/invalid messages)
- TCP listener with concurrent connections
- OTEL HTTP receiver and request parsing
- Dispatcher channel flow
- Export with batching and retry

### Performance Tests

- Throughput: 10k+ msgs/sec
- Latency: <2ms p99 from receive to export
- Memory: <100MB baseline
- CPU: <5% under normal load

---

## Dependencies

### Direct Dependencies

```
tokio = { version = "1.35", features = ["full"] }
axum = "0.7"
tonic = "0.10"    # gRPC (for future)
reqwest = { version = "0.11", features = ["json"] }
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
chrono = { version = "0.4", features = ["serde"] }
uuid = { version = "1.6", features = ["v4", "serde"] }
anyhow = "1.0"
thiserror = "1.0"
tracing = "0.1"
tracing-subscriber = "0.3"
bytes = "1.5"
log4tc-core = { path = "../log4tc-core" }
log4tc-ads = { path = "../log4tc-ads" }
log4tc-otel = { path = "../log4tc-otel" }
```

### Why Tokio?

- Industry standard async runtime for Rust
- Mature, well-tested, high performance
- Great support for networking (TCP, HTTP)
- Integrates well with axum, tonic, reqwest

---

## Deployment

### Windows Service

```rust
// Windows service integration (windows-rs)
use windows_service::service::{ServiceControl, ServiceStatus, ServiceType, ServiceState};

// Allows systemctl/sc.exe to manage the service
```

### Docker

```dockerfile
FROM rust:latest as builder
WORKDIR /app
COPY . .
RUN cargo build --release

FROM debian:bookworm
COPY --from=builder /app/target/release/log4tc-service /usr/local/bin/
EXPOSE 16150 4318
ENTRYPOINT ["log4tc-service"]
```

### Systemd Unit

```ini
[Unit]
Description=Log4TC Logging Service
After=network.target

[Service]
Type=simple
User=log4tc
ExecStart=/usr/local/bin/log4tc-service
Restart=on-failure
RestartSec=10

[Install]
WantedBy=multi-user.target
```

---

## Future Extensibility

### Plugin System (Planned)

```rust
pub trait OutputPlugin: Send + Sync {
    async fn send(&self, record: &LogRecord) -> Result<()>;
}

// Examples:
// - GraylogOutput (GELF/UDP)
// - InfluxDBOutput (Line protocol)
// - ElasticsearchOutput (JSON bulk API)
// - CustomHTTPOutput (webhook)
```

### Protocol Extensions

- gRPC receiver (using tonic)
- Kafka consumer
- File tailer
- Syslog receiver

---

**Document Status**: Complete  
**Rust Edition**: 2021  
**MSRV**: 1.70+  
**Last Review**: March 31, 2026

# Technical Specifications - Log4TC System

## 1. Network Communication

### Current Protocol: ADS (Automation Device Specification)

**Overview**:
- Proprietary protocol by Beckhoff for TwinCAT communication
- Binary protocol with compact message format
- Port: **16150** (default for Log4TC)
- Server Name: "Log4Tc" (AMS registration)

**Implementation**:
- Library: `Beckhoff.TwinCAT.Ads` v6.1.298
- Receiver class: `AdsLogReceiver` (inherits from `AdsServer`)
- Async protocol: Uses `OnWriteAsync` method to receive log data

**Binary Format Details**:

```
FRAME STRUCTURE:
┌─────────────────────────────────────────┐
│ Source (AMS Address)                    │
│ - NetId (network identifier)            │
│ - Port (ADS port)                       │
└─────────────────────────────────────────┘
        ↓
┌─────────────────────────────────────────┐
│ Index Group: (variable)                 │
│ Index Offset: (variable)                │
│ Data (binary LogEntry)                  │
└─────────────────────────────────────────┘

BINARY LOG ENTRY (Little-endian):
┌──────────────────────────────────┐
│ Version:     1 byte  [0x01]      │
├──────────────────────────────────┤
│ Message:     String (length prefixed)
├──────────────────────────────────┤
│ Logger:      String              │
├──────────────────────────────────┤
│ Level:       1 byte (0-5)        │
│              0=Trace, 1=Debug    │
│              2=Info, 3=Warn      │
│              4=Error, 5=Fatal    │
├──────────────────────────────────┤
│ PlcTimestamp:     8 bytes (FILETIME)   │
│ ClockTimestamp:   8 bytes (FILETIME)   │
├──────────────────────────────────┤
│ TaskIndex:   4 bytes (int32)     │
│ TaskName:    String              │
│ TaskCycleCounter: 4 bytes (uint32)│
├──────────────────────────────────┤
│ AppName:     String              │
│ ProjectName: String              │
│ OnlineChangeCount: 4 bytes       │
├──────────────────────────────────┤
│ Arguments/Context Section:       │
│   [Type:1 | ArgIdx:1 | Value]   │
│   [Type:2 | Scope:1 | Name | Value]
│   ... (repeating until Type:0)   │
└──────────────────────────────────┘

STRING ENCODING:
- Encoding: UTF-8 with CodePages provider
- Format: [Length:2 bytes (uint16)] + [Data:UTF-8 bytes]
- RegisterProvider: CodePagesEncodingProvider.Instance

FILETIME:
- 8-byte Windows FILETIME format
- 100-nanosecond intervals since 1601-01-01
- Parsed by reader.ReadFiletime() helper method
```

### Proposed Protocol: OpenTelemetry (OTEL)

**Overview**:
- Open, standardized telemetry protocol
- Supports both gRPC and HTTP/protobuf transport
- Wide ecosystem support
- Better cloud/observability integration

**Options**:

#### Option A: OTLP gRPC
```
Service listens on: :4317 (default OTEL gRPC port)
Protocol: gRPC with protobuf
Sent by TwinCAT: LogsServiceClient in OTEL SDK
Advantages: Efficient binary, bidrectional streaming
Disadvantages: Requires gRPC library in PLC
```

#### Option B: OTLP HTTP/JSON
```
Service listens on: :4318 (default OTEL HTTP port)
Protocol: HTTP POST with JSON/protobuf
Endpoint: /v1/logs
Sent by TwinCAT: HTTP client library
Advantages: Simpler PLC library, standard HTTP
Disadvantages: Larger payload, slightly higher latency
```

#### Option C: OTLP HTTP/Protobuf
```
Same as Option B but binary protobuf instead of JSON
Advantages: Compact payload, standard protocol
Disadvantages: Requires protobuf support in PLC
```

**Recommendation**: **Option B (OTLP HTTP/JSON)**
- Simplest PLC library implementation
- Standard HTTP - works everywhere
- JSON is human-readable for debugging
- Still efficient enough for PLC data volumes

**OTLP Log Record Mapping**:
```rust
pub struct LogRecord {
    timestamp: SystemTime,              // From ClockTimestamp
    body: Value,                        // From Message (formatted)
    severity_number: i32,               // From Level (0-5)
    resource: Resource,                 // App/ProjectName
    scope: InstrumentationScope,        // Logger name
    attributes: HashMap<String, Value>, // Arguments + Context
}

RESOURCE ATTRIBUTES:
{
    "service.name": ProjectName,
    "service.instance.id": AppName,
    "host.name": Hostname,
    "process.pid": TaskIndex,
    "process.command_line": TaskName,
}

SCOPE ATTRIBUTES:
{
    "logger.name": Logger,
}

LOG RECORD ATTRIBUTES:
{
    "plc.timestamp": PlcTimestamp (ISO8601),
    "task.cycle": TaskCycleCounter,
    "online.changes": OnlineChangeCount,
    "source.address": Source (AMS address),
    // Plus message template arguments...
}
```

---

## 2. Message Template Format

**Standard**: [Message Templates Org](https://messagetemplates.org/)

**Syntax**:
```
"{templateName} = {templateValue}, {namedProperty}"
```

**Examples**:
```
"Motor {motorId} temperature is {temperature}°C"
"Axis {axis} moved to {position:F2} at {timestamp:O}"
"Error {code}: {message}"
```

**Parser Implementation**:
- File: `/d/Projects/Open Source/log4TC/source/Log4Tc/Log4Tc.Model/Message/MessageFormatter.cs`
- Extracts named placeholders from message template
- Maps argument indices to placeholder names
- Formats final message string

**Rust Implementation**:
```rust
pub struct MessageTemplate {
    template: String,
    properties: Vec<String>, // ["motorId", "temperature", ...]
}

impl MessageTemplate {
    pub fn format(&self, args: &HashMap<usize, serde_json::Value>) -> String {
        // Replace {motorId} with args[0], {temperature} with args[1], etc.
    }
}
```

---

## 3. Data Models

### LogLevel Enumeration
```csharp
// Current .NET implementation
public enum LogLevel
{
    Trace = 0,
    Debug = 1,
    Information = 2,
    Warning = 3,
    Error = 4,
    Critical = 5,
    None = 6
}
```

**Rust Equivalent**:
```rust
#[repr(u8)]
pub enum LogLevel {
    Trace = 0,
    Debug = 1,
    Information = 2,
    Warning = 3,
    Error = 4,
    Critical = 5,
    None = 6,
}

impl From<u8> for LogLevel {
    fn from(val: u8) -> Self {
        match val {
            0 => LogLevel::Trace,
            1 => LogLevel::Debug,
            // ...
        }
    }
}
```

### LogEntry Structure
```rust
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct LogEntry {
    // Source identification
    pub source: String,              // AMS address
    pub hostname: String,            // PLC hostname
    
    // Message content
    pub message: String,             // Template string
    pub logger: String,              // Logger name
    pub level: LogLevel,             // Severity
    
    // Timestamps
    pub plc_timestamp: DateTime<Utc>,    // PLC-side time
    pub clock_timestamp: DateTime<Utc>,  // System clock time
    
    // Task metadata
    pub task_index: i32,             // Task ID
    pub task_name: String,           // Task name
    pub task_cycle_counter: u32,     // Cycle count
    
    // Application metadata
    pub app_name: String,            // Application name
    pub project_name: String,        // Project name
    pub online_change_count: u32,    // Online changes
    
    // Variable data
    pub arguments: HashMap<usize, serde_json::Value>,      // Positional args
    pub context: HashMap<String, serde_json::Value>,       // Context props
}
```

### Context Scope Levels
```
Scope 0: Global context (applies to all logs)
Scope 1: Task context (applies to logs from this task)
Scope 2: Session context (applies to logs in this session)
Scope 3: Request context (applies to logs in this request)
```

---

## 4. Plugin System Architecture

### Plugin Trait Definition
```rust
pub trait LogOutput: Send + Sync {
    /// Called when plugin is loaded and configured
    async fn initialize(&mut self, config: serde_json::Value) -> Result<()>;
    
    /// Called for each log entry
    async fn output(&self, entry: &LogEntry) -> Result<()>;
    
    /// Called when service is shutting down
    async fn shutdown(&mut self) -> Result<()>;
}

pub struct PluginInfo {
    name: String,
    version: String,
    output_type: String,
}
```

### Plugin Discovery and Loading

**Static Registration** (recommended for Rust):
```rust
// In main.rs or plugins module
fn create_plugins(config: &AppSettings) -> Vec<Box<dyn LogOutput>> {
    vec![
        Box::new(NLogOutput::new()),
        Box::new(GraylogOutput::new()),
        Box::new(InfluxDbOutput::new()),
        Box::new(SqlOutput::new()),
    ]
}
```

**Dynamic Loading** (advanced):
- Use `libloading` crate to load compiled .so/.dll plugins at runtime
- Requires stable ABI definition
- More complex, but allows extensibility

### Plugin Configuration Example
```json
{
  "Outputs": [
    {
      "Type": "nlog",
      "ConfigFile": "/etc/log4tc/nlog.config"
    },
    {
      "Type": "graylog",
      "Host": "localhost",
      "Port": 12201,
      "Protocol": "UDP",
      "Facility": "LOG4TC"
    },
    {
      "Type": "influxdb",
      "Url": "http://localhost:8086",
      "Database": "logs",
      "Bucket": "twincat",
      "Organization": "myorg",
      "AuthToken": "secret_token"
    },
    {
      "Type": "sql",
      "Provider": "MSSQL",
      "ConnectionString": "Server=localhost;Database=logs;User=sa;",
      "TableName": "LogEntries"
    }
  ]
}
```

---

## 5. Output Plugins Details

### 1. NLog Output
```rust
pub struct NLogOutput {
    // Configuration
    config_file: PathBuf,
    // Internal state
    http_client: HttpClient,
}

impl LogOutput for NLogOutput {
    async fn output(&self, entry: &LogEntry) -> Result<()> {
        // Send to NLog configured targets
        // Can be files, databases, APIs, etc.
    }
}
```

**Configuration**:
```json
{
  "Type": "nlog",
  "ConfigFile": "/path/to/nlog.config",
  "LoggerName": "Log4TC.Service"
}
```

### 2. Graylog Output
```rust
pub struct GraylogOutput {
    host: String,
    port: u16,
    facility: String,
    client: UdpSocket,
}

impl LogOutput for GraylogOutput {
    async fn output(&self, entry: &LogEntry) -> Result<()> {
        let gelf_message = entry.to_gelf();
        let compressed = gzip::compress(&gelf_message)?;
        self.client.send_to(&compressed, (self.host.clone(), self.port))?;
    }
}
```

**GELF Format**:
```json
{
  "version": "1.1",
  "host": "plc-01",
  "timestamp": 1672531200.123,
  "level": 3,
  "short_message": "Motor temperature is 85°C",
  "full_message": "Motor temperature is 85°C at 2023-01-01T12:00:00Z",
  "facility": "log4tc",
  "_logger": "MotorController",
  "_task_name": "MotorTask",
  "_task_index": 1,
  "_motor_id": "MOT-001",
  "_temperature": 85.5
}
```

### 3. InfluxDB Output
```rust
pub struct InfluxDbOutput {
    url: String,
    bucket: String,
    organization: String,
    token: String,
    client: HttpClient,
}

impl LogOutput for InfluxDbOutput {
    async fn output(&self, entry: &LogEntry) -> Result<()> {
        let point = entry.to_influx_point();
        self.client.write_point(&point).await?;
    }
}
```

**Line Protocol Format**:
```
log4tc,host=plc-01,logger=MotorController,level=warning task_index=1i,task_cycle=12345u,temperature=85.5 1672531200000000000
```

### 4. SQL Output
```rust
pub struct SqlOutput {
    connection_pool: ConnectionPool,
    table_name: String,
}

impl LogOutput for SqlOutput {
    async fn output(&self, entry: &LogEntry) -> Result<()> {
        let query = format!(
            "INSERT INTO {} (timestamp, logger, level, message, ...) VALUES (?, ?, ?, ?, ...)",
            self.table_name
        );
        self.connection_pool.execute(&query, entry).await?;
    }
}
```

**Database Schema**:
```sql
CREATE TABLE LogEntries (
    id INT PRIMARY KEY IDENTITY(1,1),
    timestamp DATETIME2 NOT NULL,
    plc_timestamp DATETIME2 NOT NULL,
    source NVARCHAR(255),
    hostname NVARCHAR(255),
    message NVARCHAR(MAX),
    logger NVARCHAR(255),
    level INT,
    task_index INT,
    task_name NVARCHAR(255),
    task_cycle_counter BIGINT,
    app_name NVARCHAR(255),
    project_name NVARCHAR(255),
    arguments NVARCHAR(MAX), -- JSON
    context NVARCHAR(MAX),   -- JSON
    created_at DATETIME2 DEFAULT GETDATE()
);

CREATE INDEX idx_timestamp ON LogEntries(timestamp);
CREATE INDEX idx_logger ON LogEntries(logger);
CREATE INDEX idx_level ON LogEntries(level);
```

---

## 6. Service Architecture

### Async Runtime Structure
```rust
#[tokio::main]
async fn main() -> Result<()> {
    // Initialize
    let config = load_config()?;
    let log_output = init_logging(&config)?;
    
    // Create service components
    let receiver = OtelReceiver::new(config.receiver)?;
    let dispatcher = LogDispatcher::new(config.outputs)?;
    
    // Start service tasks
    let receiver_handle = tokio::spawn(receiver.listen());
    let dispatcher_handle = tokio::spawn(dispatcher.run());
    
    // Wait for shutdown signal
    signal::ctrl_c().await?;
    
    // Graceful shutdown
    receiver.shutdown().await?;
    dispatcher.shutdown().await?;
    
    Ok(())
}
```

### Component Interaction Flow
```
┌─────────────────────────┐
│  OTEL Log Receiver      │
│  (HTTP/gRPC listener)   │
└────────────┬────────────┘
             │ ParsedLogEntry
             ▼
┌─────────────────────────────────┐
│  Log Entry Buffer               │
│  (AsyncChannel / Bounded Queue) │
└────────────┬────────────────────┘
             │ LogEntry
             ▼
┌─────────────────────────────────┐
│  Log Dispatcher                 │
│  - Filters                      │
│  - Enriches (context)           │
│  - Routes to outputs            │
└──┬──────┬─────────┬──────┬──────┘
   │      │         │      │
   ▼      ▼         ▼      ▼
  NLog  Graylog  InfluxDB SQL
```

### Configuration Hot-Reload
```rust
pub struct ConfigWatcher {
    config_path: PathBuf,
    tx: mpsc::Sender<AppSettings>,
}

impl ConfigWatcher {
    pub async fn watch(&self) -> Result<()> {
        let (tx, mut rx) = watch::channel(self.load_config()?);
        
        loop {
            if self.config_changed().await? {
                let new_config = self.load_config()?;
                tx.send(new_config)?;
            }
            tokio::time::sleep(Duration::from_secs(5)).await;
        }
    }
}
```

---

## 7. Performance Characteristics

### Current System (ADS-based)
- **Latency**: ~1-5ms from PLC to service
- **Throughput**: 1000-10000 logs/second (depending on size)
- **Memory**: ~50-100MB running (base service)
- **CPU**: <1% idle, 5-10% under load
- **Protocol Overhead**: ~20% (binary encoding is efficient)

### Expected Rust Implementation
- **Latency**: ~0.5-2ms (Rust async runtime is faster)
- **Throughput**: 10000-50000 logs/second (better parallelism)
- **Memory**: ~20-50MB running (Rust is more efficient)
- **CPU**: <0.5% idle, 2-5% under load
- **Protocol Overhead**: ~15% (OTEL HTTP/JSON) to ~10% (HTTP/protobuf)

### Optimization Opportunities
1. Message buffering/batching before output
2. Async output operations (don't block receiver)
3. Connection pooling for database outputs
4. Compression for network transport
5. Partial message parsing (streaming)

---

## 8. Error Handling Strategy

### Error Categories

```rust
pub enum Log4TcError {
    // Protocol errors
    ProtocolError(String),
    InvalidFormat(String),
    
    // Network errors
    NetworkError(String),
    ConnectionError(String),
    
    // Configuration errors
    ConfigError(String),
    InvalidConfig(String),
    
    // Output errors
    OutputError(String),
    DatabaseError(String),
    
    // System errors
    IOError(String),
    Internal(String),
}

impl From<std::io::Error> for Log4TcError {
    fn from(e: std::io::Error) -> Self {
        Log4TcError::IOError(e.to_string())
    }
}

// Similar implementations for other error types
```

### Recovery Strategies
- **Receiver errors**: Log error, continue listening
- **Parser errors**: Log malformed message, skip it
- **Output errors**: Implement retry with exponential backoff
- **Database errors**: Queue message for later delivery, or drop if queue full
- **Critical errors**: Log and shutdown gracefully

---

## 9. Security Considerations

### Current State
- No authentication on ADS receiver
- Messages sent in clear (binary, not encrypted)
- Runs as Windows service (elevated privileges)

### Recommendations for OTEL
1. **Authentication**: 
   - Use HTTP Basic Auth or API keys
   - Store credentials in secure config location
   
2. **Transport Security**:
   - Enforce TLS/HTTPS for OTEL endpoint
   - Use self-signed certificates for internal networks
   
3. **Authorization**:
   - Validate source IP addresses
   - Implement log filtering by source
   
4. **Data Protection**:
   - Scrub sensitive data before output (PII)
   - Implement audit logging of configuration changes
   
5. **Service Isolation**:
   - Run with minimal required privileges
   - Use Windows service user account (not SYSTEM)

---

## 10. Deployment & Installation

### Current (Windows Service)
```
Log4TC-24.01.17.msi
├── Service executable
├── Configuration files
├── NLog configuration
└── WiX installer
```

### Proposed (Rust)
```
log4tc-service.exe (single executable)
- No dependencies on .NET runtime
- Smaller download size
- Easier installation
- Cross-platform potential

Installation options:
1. MSI (via WiX or alternative)
2. Standalone executable
3. Zip archive with setup script
```

### Windows Service Setup (Rust)
```rust
use windows_service::service::{ServiceControl, ServiceControlHandler, ServiceStatus, ServiceState};
use windows_service::service_dispatcher;

service_dispatcher::run("Log4TcService", run_service)?;

fn run_service(_arguments: Vec<OsString>) -> ServiceResult<()> {
    let (shutdown_tx, shutdown_rx) = mpsc::channel(1);
    
    let _service_handler = ServiceControlHandler::register("Log4TcService", |control_event| {
        match control_event {
            ServiceControl::Stop | ServiceControl::Shutdown => {
                shutdown_tx.send(()).ok();
                ServiceControlHandler::ServiceStatus(ServiceStatus {
                    service_type: ServiceType::Own,
                    current_state: ServiceState::StopPending,
                    // ...
                })
            }
        }
    })?;
    
    // Run async service with tokio
    tokio::runtime::Runtime::new()?.block_on(async {
        run_async_service(shutdown_rx).await
    })
}
```

---

## 11. Testing Strategy

### Unit Tests
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_log_level_parsing() {
        assert_eq!(LogLevel::from(0u8), LogLevel::Trace);
        assert_eq!(LogLevel::from(4u8), LogLevel::Error);
    }

    #[tokio::test]
    async fn test_message_template_formatting() {
        let template = MessageTemplate::parse(
            "Motor {motorId} temperature is {temperature}°C"
        ).unwrap();
        let args = vec![(0, "MOT-01".into()), (1, "85.5".into())];
        assert_eq!(template.format(&args), "Motor MOT-01 temperature is 85.5°C");
    }

    #[tokio::test]
    async fn test_dispatcher_routes_correctly() {
        let dispatcher = LogDispatcher::new(mock_config()).await.unwrap();
        let entry = create_test_log_entry();
        dispatcher.dispatch(entry).await.unwrap();
        // Verify output was called
    }
}
```

### Integration Tests
```rust
#[tokio::test]
async fn test_end_to_end_http_to_output() {
    // Start mock HTTP server listening for OTEL
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    
    // Start service
    let service = LogService::new(config).await.unwrap();
    service.run().await.unwrap();
    
    // Send HTTP request to OTEL endpoint
    let client = reqwest::Client::new();
    let response = client.post(format!("http://{}/v1/logs", addr))
        .json(&test_log_record())
        .send()
        .await
        .unwrap();
    
    assert_eq!(response.status(), 200);
    
    // Verify output plugin received the log
    // (e.g., check database, verify file written, etc.)
}
```

---

## Summary of Technical Specifications

| Aspect | Current | Proposed |
|--------|---------|----------|
| **Protocol** | ADS (port 16150) | OTEL HTTP/JSON (port 4318) |
| **Language** | C# (.NET 6.0) | Rust (tokio async) |
| **Message Format** | Binary (version 1) | JSON/Protobuf (OTEL) |
| **Plugin System** | .NET interfaces | Rust traits |
| **Configuration** | JSON (appsettings) | JSON (serde) |
| **Service Host** | .NET host | windows-rs or systemd |
| **Logging** | Serilog | tracing |
| **Testing** | xUnit | cargo test |
| **Latency** | 1-5ms | <2ms (target) |
| **Throughput** | 1k-10k logs/sec | 10k-50k logs/sec (target) |


# Log4TC Architecture Design

## 1. Executive Summary

Log4TC is migrating from a .NET Windows Service architecture to a modern Rust-based implementation with OpenTelemetry Protocol (OTLP) as the primary output mechanism. This migration addresses three critical objectives:

- **Performance**: Reduce memory footprint and increase throughput to handle 10,000+ logs per second with <2ms latency
- **Maintainability**: Consolidate 4 separate output plugins into a single unified OTLP export pathway with vendor agnostic routing
- **Modern Observability**: Adopt industry-standard OpenTelemetry ecosystem, enabling integration with any OTEL-compatible backend (Datadog, New Relic, Jaeger, Grafana Loki, etc.)

The TwinCAT PLC library remains functionally unchanged; only performance optimizations are planned.

---

## 2. Current Architecture

### 2.1 Overview

The existing .NET-based log4TC consists of an application receiving logs from TwinCAT PLCs over the ADS binary protocol, deserializing them, and dispatching to multiple output plugins.

### 2.2 Component Diagram (ASCII)

```
┌─────────────────────────────────────────────────────────────────┐
│                      TwinCAT PLC                                │
│    (PLC Library v0.2.3 - Dual-buffer pattern)                   │
│    Sends: Binary Protocol v1 over ADS Port 16150               │
└────────────────────────┬────────────────────────────────────────┘
                         │
                         │ ADS Binary (16150)
                         ▼
┌─────────────────────────────────────────────────────────────────┐
│                    .NET Application                             │
│                                                                  │
│  ┌────────────────────────────────────────────────────────────┐ │
│  │ ADS Log Receiver (TwinCAT.Ads.Server)                      │ │
│  │ - Listens on port 16150                                    │ │
│  │ - Deserializes binary protocol v1                          │ │
│  │ - Parses 44 LogEntry properties                            │ │
│  │ - Handles 16 object types + context                        │ │
│  └────────────────┬─────────────────────────────────────────┘ │
│                   │ LogEntryEventArgs                          │
│                   ▼                                             │
│  ┌────────────────────────────────────────────────────────────┐ │
│  │ Log Dispatcher Service (TPL Dataflow)                      │ │
│  │ - BufferBlock<List<LogEntry>>                              │ │
│  │ - ActionBlock for async processing                         │ │
│  │ - Parallel output dispatch                                 │ │
│  │ - Hot-reload config from appsettings.json                  │ │
│  └────────────────┬──────────────────────────────────────────┘ │
│                   │                                             │
│    ┌──────────────┼──────────────┬──────────────┐              │
│    │              │              │              │              │
│    ▼              ▼              ▼              ▼              │
│  ┌────────┐  ┌────────┐  ┌────────┐  ┌────────┐              │
│  │  NLog  │  │Graylog │  │InfluxDB│  │  SQL   │              │
│  │ Output │  │ Output │  │ Output │  │ Output │              │
│  │ Plugin │  │ Plugin │  │ Plugin │  │ Plugin │              │
│  └───┬────┘  └───┬────┘  └───┬────┘  └───┬────┘              │
│      │           │           │           │                    │
│      ▼           ▼           ▼           ▼                    │
└──────┼───────────┼───────────┼───────────┼────────────────────┘
       │           │           │           │
       ▼           ▼           ▼           ▼
    ┌────┐    ┌────────┐   ┌────────┐  ┌────┐
    │Text│    │Graylog │   │InfluxDB│  │ DB │
    │File│    │ Server │   │ Server │  │    │
    └────┘    └────────┘   └────────┘  └────┘

Legend:
  → = Async message flow
  ▼ = Component dependency
```

### 2.3 Key Components

| Component | Technology | Responsibility |
|-----------|-----------|-----------------|
| **ADS Log Receiver** | TwinCAT.Ads (C# Server) | Listen on port 16150; deserialize binary protocol v1; emit LogEntry events |
| **Log Dispatcher** | TPL Dataflow | Buffer incoming logs; async dispatch to outputs; config hot-reload |
| **Output Plugins** | IPlugin interface | 4 implementations (NLog, Graylog, InfluxDB, SQL); vendor-specific formatting |
| **Model** | C# POCO | LogEntry with 44 properties; Arguments dict; Context dict |
| **Service Host** | .NET Generic Host | DI container; config management |

### 2.4 Data Model (LogEntry)

**Properties** (44 total):
- Core: `Message`, `Logger`, `Level` (Fatal, Error, Warn, Info, Debug, Trace)
- Timestamps: `PlcTimestamp` (Windows FILETIME), `ClockTimestamp` (received time)
- Task Info: `TaskIndex`, `TaskName`, `TaskCycleCounter`, `OnlineChangeCount`
- App Info: `AppName`, `ProjectName`, `Source` (NetId), `Hostname`
- Arguments: `Arguments` (Dict<int, object>) - positional message template vars
- Context: `Context` (Dict<string, object>) - structured scopes

### 2.5 Binary Protocol v1

- **Version**: 1 byte header
- **Core Fields**: Message, Logger, LogLevel, PlcTimestamp, ClockTimestamp, TaskIndex, TaskName, TaskCycleCounter, AppName, ProjectName, OnlineChangeCount
- **Arguments**: Type 1 (byte + arg index + object value) - up to 16 argument types
- **Context**: Type 2 (scope + name + object value)
- **String Encoding**: CP1252 (code page)
- **Timestamp Format**: Windows FILETIME (100ns intervals since 1601)

### 2.6 Configuration (appsettings.json)

```json
{
  "Logging": { "LogLevel": { "Default": "Information" } },
  "Outputs": [
    { "Type": "nlog" },
    { "Type": "graylog", "Address": "localhost", "Port": 12201 },
    { "Type": "influxdb", "Url": "http://localhost:8086", "Bucket": "logs" },
    { "Type": "sql", "ConnectionString": "Server=..." }
  ]
}
```

---

## 3. Target Architecture

### 3.1 Overview

The new Rust-based architecture replaces all 4 output plugins with a unified OTLP export pathway. The service maintains the same ADS receiver interface but routes all logs through a centralized OpenTelemetry semantic mapping layer, enabling flexible backend routing through a separate OTEL Collector deployment.

### 3.2 Component Diagram (ASCII)

```
┌─────────────────────────────────────────────────────────────────┐
│                      TwinCAT PLC                                │
│    (PLC Library v0.2.3 - UNCHANGED)                             │
│    Sends: Binary Protocol v1 over ADS Port 16150               │
└────────────────────────┬────────────────────────────────────────┘
                         │
                         │ ADS Binary (16150)
                         ▼
┌─────────────────────────────────────────────────────────────────┐
│                     Rust Standalone Binary                     │
│  (Single-threaded async runtime with tokio)                    │
│                                                                  │
│  ┌────────────────────────────────────────────────────────────┐ │
│  │ ADS Protocol Handler (log4tc-ads crate)                    │ │
│  │ - Listens on port 16150                                    │ │
│  │ - Async decode binary protocol v1                          │ │
│  │ - Emit: StructuredLogMessage                               │ │
│  └────────────────┬─────────────────────────────────────────┘ │
│                   │                                             │
│                   ▼                                             │
│  ┌────────────────────────────────────────────────────────────┐ │
│  │ Core Dispatcher (log4tc-core crate)                        │ │
│  │ - In-memory queue (async-channel)                          │ │
│  │ - Handles backpressure                                     │ │
│  │ - Parallel batch processing                                │ │
│  │ - Config hot-reload (TOML)                                 │ │
│  └────────────────┬─────────────────────────────────────────┘ │
│                   │ StructuredLogMessage                        │
│                   ▼                                             │
│  ┌────────────────────────────────────────────────────────────┐ │
│  │ OTEL Semantic Mapper (log4tc-otel crate)                   │ │
│  │ - LogRecord: message, severity, timestamp, attributes      │ │
│  │ - Attributes: task_name, app_name, logger_name, etc.      │ │
│  │ - Context propagation                                      │ │
│  │ - Resource: service.name, service.version, host.name       │ │
│  └────────────────┬─────────────────────────────────────────┘ │
│                   │ OtelLogRecord                               │
│                   ▼                                             │
│  ┌────────────────────────────────────────────────────────────┐ │
│  │ OTLP Exporter (log4tc-otel crate)                          │ │
│  │ - HTTP/Protobuf to gRPC endpoint                           │ │
│  │ - Configurable batch size & timeout                        │ │
│  │ - Retry with exponential backoff                           │ │
│  │ - Dead-letter queue for failed exports                     │ │
│  └────────────────┬─────────────────────────────────────────┘ │
│                   │ OTLP/Protobuf                              │
└───────────────────┼──────────────────────────────────────────┘
                    │
                    ▼
┌─────────────────────────────────────────────────────────────────┐
│         OpenTelemetry Collector (Separate Deployment)          │
│                                                                  │
│  Receivers:                                                     │
│  - OTLP gRPC (4317)                                            │
│  - OTLP HTTP (4318)                                            │
│                                                                  │
│  Processors:                                                    │
│  - Batch processor                                              │
│  - Sampler                                                      │
│  - Attribute processor (enrich)                                │
│                                                                  │
│  Exporters:                                                     │
│  - Datadog         - New Relic      - Jaeger                   │
│  - Grafana Loki    - Splunk         - Prometheus               │
│  - File/Console    - Custom (user-defined)                     │
└─────────────────────────────────────────────────────────────────┘
```

### 3.3 New Components

| Component | Crate | Technology | Responsibility |
|-----------|-------|-----------|-----------------|
| **ADS Protocol Handler** | `log4tc-ads` | tokio + bytes | Async ADS server; binary v1 deserialization |
| **Core Dispatcher** | `log4tc-core` | tokio + async-channel | Queue management; backpressure; batch processing |
| **OTEL Mapper** | `log4tc-otel` | opentelemetry-rs SDK | LogRecord construction; semantic attribute mapping |
| **OTLP Exporter** | `log4tc-otel` | opentelemetry-proto + tonic | Protobuf encoding; gRPC transmission; retry logic |
| **Service Host** | `log4tc-service` | windows-rs | Windows Service integration; config; lifecycle |

---

## 4. Technology Stack

### 4.1 Core Runtime

**tokio** (async-std alternative: not chosen)
- Industry-standard async runtime for Rust
- Excellent Windows support with IOCP integration
- Battle-tested in production (Discord, Cloudflare, etc.)
- Multi-threaded scheduler with configurable thread pool
- Justification: Required for high-throughput concurrent log processing; native Windows event loop integration

### 4.2 Windows Service Integration

**windows-rs** (winapi alternative: deprecated)
- Modern Windows FFI bindings (Microsoft-maintained)
- Zero-cost abstractions over Win32 API
- ServiceMain, RegisterServiceCtrlHandlerEx for service lifecycle
- Event logging to Windows Event Log
- Justification: Future-proof maintenance; reduces unsafe code; Microsoft official support

### 4.3 OpenTelemetry Stack

**opentelemetry** (otel = OpenTelemetry SDK)
- `opentelemetry`: Core API and SDK
- `opentelemetry-proto`: Protobuf message definitions
- `tonic`: gRPC transport (async, built on tokio)
- `opentelemetry-otlp`: OTLP exporter (HTTP and gRPC)
- Justification: Industry-standard observability layer; vendor-neutral; enables seamless backend switching; future-proof ecosystem growth

### 4.4 Serialization

**prost** (protobuf code gen)
- Used by tonic/opentelemetry for OTLP message encoding
- Efficient binary protocol
- Justification: OTLP standard requires protobuf; prost is the de facto Rust standard

### 4.5 Configuration

**toml** (serde + toml crate)
- Replaces JSON appsettings.json
- Human-readable; supports tables and arrays
- Hot-reload on file change detection
- Justification: More ergonomic than JSON; native table support for OTEL exporter configs

### 4.6 Async Utilities

**async-channel**
- Multi-producer, multi-consumer queue
- Backpressure support
- Justification: Simpler than crossbeam for single async use case; channel-based architecture

**bytes**
- Zero-copy buffer handling for binary protocol deserialization
- Justification: Efficient memory management; reduces allocations in hot path

### 4.7 Logging & Diagnostics

**tracing** + **tracing-subscriber**
- Structured logging with spans and events
- Integration with service diagnostics
- Justification: Instrumentation-grade observability; low overhead; structured data

---

## 5. Crate/Module Structure

### 5.1 Workspace Layout

```
log4tc-rust/
├── Cargo.workspace.toml
├── Cargo.lock
│
├── crates/
│   ├── log4tc-ads/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── server.rs           # ADS server implementation
│   │       ├── protocol.rs         # Binary v1 deserialization
│   │       ├── types.rs            # StructuredLogMessage definition
│   │       └── error.rs            # Error handling
│   │
│   ├── log4tc-core/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── dispatcher.rs       # Queue & batch processor
│   │       ├── config.rs           # TOML configuration
│   │       ├── message.rs          # Internal message types
│   │       └── error.rs
│   │
│   ├── log4tc-otel/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── mapper.rs           # Semantic mapping (ADS -> OTEL)
│   │       ├── resource.rs         # OTEL Resource builder
│   │       ├── exporter.rs         # OTLP gRPC/HTTP export
│   │       ├── batch.rs            # Batch accumulation
│   │       └── error.rs
│   │
│   └── log4tc-service/
│       ├── Cargo.toml
│       └── src/
│           ├── main.rs             # Entry point
│           ├── service.rs          # Windows Service wrapper
│           ├── lifecycle.rs        # Startup/shutdown orchestration
│           ├── config.rs           # Config file loading
│           └── error.rs
│
├── tests/
│   ├── integration/
│   │   ├── protocol_v1.rs          # Binary protocol test vectors
│   │   ├── e2e_ads_to_otel.rs      # End-to-end flow
│   │   └── config_hotload.rs
│   │
│   └── fixtures/
│       ├── sample-logs.bin         # Test protocol buffers
│       └── config.toml
│
├── docs/
│   ├── architecture.md             # This file
│   ├── protocol_v1.md              # Binary protocol reference
│   ├── otel_mapping.md             # Semantic mapping spec
│   └── build_deploy.md             # Build & deployment guide
│
├── examples/
│   └── plc-simulator/              # Mock PLC for testing
│       ├── Cargo.toml
│       └── src/main.rs
│
└── README.md
```

### 5.2 Crate Responsibilities

#### **log4tc-ads**
Responsible for receiving and deserializing log data from PLCs.

**Key Exports:**
- `struct AdsServer`: Listens on port 16150
- `struct StructuredLogMessage`: Deserialized log (44 fields)
- `fn decode_protocol_v1(bytes) -> Result<StructuredLogMessage>`

**Dependencies:**
- tokio (async runtime)
- bytes (buffer handling)
- tracing (diagnostics)

**Non-dependencies:**
- opentelemetry (to keep protocol decoding independent)

#### **log4tc-core**
Orchestrates message flow and manages configuration.

**Key Exports:**
- `struct Dispatcher`: Multi-producer queue + batch processor
- `struct Config`: Parsed TOML configuration
- `trait MessageHandler`: Trait for batch callbacks

**Dependencies:**
- tokio
- async-channel
- serde + toml
- tracing

**Non-dependencies:**
- opentelemetry (to keep generic for possible future plugin outputs)

#### **log4tc-otel**
Maps internal messages to OpenTelemetry semantics and exports via OTLP.

**Key Exports:**
- `struct OtelMapper`: Converts StructuredLogMessage -> LogRecord
- `struct OtlpExporter`: Handles gRPC transmission
- `enum SeverityLevel`: Maps Log4TC levels to OTEL

**Dependencies:**
- opentelemetry
- opentelemetry-proto
- tonic
- prost
- tokio
- tracing

#### **log4tc-service**
Windows Service host and main entry point.

**Key Exports:**
- `fn main()`: Service entry
- `struct ServiceRunner`: Orchestrates ADS + Dispatcher + Exporter

**Dependencies:**
- windows-rs
- tokio
- log4tc-ads, log4tc-core, log4tc-otel
- tracing
- tracing-subscriber

---

## 6. Component Interfaces

### 6.1 Core Traits

#### **log4tc-ads Traits**

```rust
// Protocol handler: ADS OnWrite callback
pub trait OnWriteHandler: Send + Sync {
    async fn handle_write(
        &self,
        target: AmsAddress,
        index_group: u32,
        index_offset: u32,
        data: Bytes,
    ) -> Result<(), ProtocolError>;
}

// Log event emission
pub trait LogEmitter {
    fn emit(&self, log: StructuredLogMessage);
}
```

#### **log4tc-core Traits**

```rust
// Batch processor callback
pub trait BatchHandler: Send + Sync {
    async fn handle_batch(&self, messages: Vec<StructuredLogMessage>) 
        -> Result<(), DispatchError>;
}

// Config watcher (hot-reload)
pub trait ConfigWatcher {
    async fn watch_file(&self, path: PathBuf) -> Result<ConfigUpdate>;
}
```

#### **log4tc-otel Traits**

```rust
// Semantic mapping
pub trait LogMapper {
    fn map_to_otel_log(&self, msg: StructuredLogMessage) 
        -> Result<LogRecord, MapError>;
}

// Export transport
pub trait OtelTransport: Send + Sync {
    async fn export_batch(&self, batch: LogRecordBatch) 
        -> Result<(), ExportError>;
}
```

### 6.2 Data Flow Structures

#### **StructuredLogMessage (log4tc-ads)**

```rust
pub struct StructuredLogMessage {
    // Core fields
    pub message: String,
    pub logger: String,
    pub level: LogLevel,
    
    // Timestamps
    pub plc_timestamp: DateTime<Utc>,
    pub clock_timestamp: DateTime<Utc>,
    
    // Task context
    pub task_index: i32,
    pub task_name: String,
    pub task_cycle_counter: u32,
    pub online_change_count: u32,
    
    // Source context
    pub source: AmsNetId,
    pub hostname: String,
    pub app_name: String,
    pub project_name: String,
    
    // Arguments and context
    pub arguments: HashMap<usize, AnyValue>,
    pub context: HashMap<String, AnyValue>,
}
```

#### **OtelLogRecord (log4tc-otel)**

```rust
pub struct OtelLogRecord {
    // OTEL standard fields
    pub timestamp: SystemTime,
    pub observed_timestamp: SystemTime,
    pub severity_number: u32,
    pub severity_text: String,
    pub body: String,
    
    // OTEL attributes (key-value pairs)
    pub attributes: HashMap<String, Value>,
    
    // Span context (for distributed tracing)
    pub trace_context: Option<TraceContext>,
    
    // Resource attributes
    pub resource: Resource,
}
```

#### **BatchMessage (log4tc-core)**

```rust
pub struct BatchMessage {
    pub messages: Vec<StructuredLogMessage>,
    pub batch_size: usize,
    pub enqueued_at: Instant,
}
```

---

## 7. Data Flow

### 7.1 Step-by-Step Processing

```
┌─────────────────────────────────────────────────────────────────┐
│ Step 1: ADS Reception (log4tc-ads)                              │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  1. TwinCAT PLC sends binary log message to ADS port 16150      │
│  2. tokio::net::TcpListener accepts connection                 │
│  3. ADS OnWrite handler receives raw bytes                      │
│  4. parse_protocol_v1() deserializes:                           │
│     - Version byte                                               │
│     - Core fields (message, logger, level, timestamps, ...)     │
│     - Arguments (type 1: variable count)                        │
│     - Context (type 2: variable count)                          │
│  5. StructuredLogMessage created                                │
│  6. Emit to dispatcher via LogEmitter trait                     │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
                         │
                         ▼
┌─────────────────────────────────────────────────────────────────┐
│ Step 2: Core Dispatch (log4tc-core)                             │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  1. Dispatcher::enqueue(message) called                          │
│  2. If queue is full, apply backpressure (wait or drop)        │
│  3. Message added to in-memory queue                            │
│  4. Check batch trigger conditions:                             │
│     - Batch size >= config.batch_size (default: 100)?          │
│     - OR timeout elapsed (default: 5s)?                         │
│     - OR queue full?                                            │
│  5. If triggered, pop batch from queue                          │
│  6. Pass BatchMessage to registered BatchHandler                │
│  7. Return acknowledgment to ADS sender                         │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
                         │
                         ▼
┌─────────────────────────────────────────────────────────────────┐
│ Step 3: OTEL Semantic Mapping (log4tc-otel)                     │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  For each StructuredLogMessage in batch:                        │
│                                                                  │
│  1. Map severity: LogLevel -> OtelLogLevel (see section 7.2)    │
│  2. Extract body: Use FormattedMessage (placeholder replacement)│
│  3. Build attributes map:                                       │
│     log.message: original template                              │
│     log.logger: logger name                                     │
│     service.name: app_name                                      │
│     service.version: project_name                               │
│     host.name: hostname                                         │
│     plc.task_name: task_name                                    │
│     plc.task_index: task_index                                  │
│     plc.task_cycle: task_cycle_counter                          │
│     plc.source_netid: source (formatted)                        │
│     ... (all context vars as attributes)                        │
│  4. Set timestamps: plc_timestamp for event time               │
│  5. Create LogRecord with Resource                              │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
                         │
                         ▼
┌─────────────────────────────────────────────────────────────────┐
│ Step 4: Batch Accumulation (log4tc-otel)                        │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  1. Accumulate LogRecords into ExportRequest                    │
│  2. Wait for batch to reach size or timeout                     │
│  3. Encode to Protobuf (opentelemetry-proto)                    │
│  4. Compress (optional, gzip)                                   │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
                         │
                         ▼
┌─────────────────────────────────────────────────────────────────┐
│ Step 5: OTLP Export (log4tc-otel)                               │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  1. Create gRPC channel to OTEL Collector                       │
│     URI: config.otel.endpoint (default: http://localhost:4317) │
│  2. Call LogsService.Export(request)                            │
│  3. Handle response:                                            │
│     - Success (200): Return OK                                  │
│     - Retryable error (503, timeout): Exponential backoff       │
│     - Permanent error (400): Log and continue                   │
│  4. On failure after retries: Store in dead-letter queue        │
│  5. Return dispatch result to dispatcher                        │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
                         │
                         ▼
          ┌──────────────────────────────┐
          │  OTEL Collector             │
          │  (User-managed deployment)  │
          │                              │
          │  Receives OTLP/gRPC logs    │
          │  Routes to backends:        │
          │  - Datadog, New Relic, ...  │
          └──────────────────────────────┘
```

### 7.2 Severity Level Mapping

| Log4TC Level | OTEL SeverityNumber | OTEL SeverityText | Example Interpretation |
|---|---|---|---|
| Trace (0) | 1 | TRACE | Detailed diagnostic info |
| Debug (1) | 5 | DEBUG | Development debugging |
| Info (2) | 9 | INFO | Informational message |
| Warn (3) | 13 | WARN | Warning condition |
| Error (4) | 17 | ERROR | Error condition |
| Fatal (5) | 21 | FATAL | Fatal condition |

---

## 8. Comparison: Old vs New

| Aspect | .NET (Current) | Rust (Target) |
|--------|---|---|
| **Runtime** | .NET 6.0 (CLR) | tokio (native async) |
| **Service Host** | Generic Host + Windows Services | windows-rs |
| **Protocol Reception** | TwinCAT.Ads (ADS Server) | log4tc-ads (custom async) |
| **Deserialization** | BinaryReader (sequential) | bytes + prost (zero-copy) |
| **Data Pipeline** | TPL Dataflow (ActionBlock) | async-channel + tokio tasks |
| **Output Plugins** | 4 separate plugins | Single unified OTLP exporter |
| **Backend Routing** | Hard-coded in code | OTEL Collector configuration |
| **Configuration** | JSON (appsettings.json) | TOML (log4tc.toml) |
| **Hot-reload** | Via IChangeToken (file watching) | inotify/ReadDirectoryChangesW |
| **Memory Footprint** | ~100-150 MB baseline | ~20-30 MB baseline (estimated) |
| **Throughput (Target)** | ~5,000 logs/sec | 10,000+ logs/sec |
| **Latency (p99)** | ~5-10 ms | <2 ms (target) |
| **Observability** | Serilog logs | tracing spans + OTEL metrics |
| **Binary Size** | ~50 MB (w/ deps) | ~5 MB (static Rust binary) |
| **Maintenance Burden** | 4 plugin codebases | 1 OTLP exporter, 1 semantic mapper |
| **Backend Flexibility** | Requires code changes | Zero code changes (config only) |

---

## 9. Non-Functional Requirements

### 9.1 Performance

| Requirement | Target | Measurement |
|---|---|---|
| **Throughput** | 10,000+ logs/sec | Sustained, single instance |
| **Latency (p50)** | <500 μs | Time from reception to dispatcher |
| **Latency (p99)** | <2 ms | Time from reception to OTLP export |
| **Memory (Baseline)** | <30 MB | Idle service on Windows |
| **Memory (Peak)** | <200 MB | Under 10k logs/sec sustained load |
| **CPU (Baseline)** | <1% | Idle service |
| **CPU (Peak)** | <30% on single core | Under 10k logs/sec (tokio work-stealing) |
| **Batch Latency** | <5 seconds | Max age of logs before export |

### 9.2 Reliability

| Requirement | Implementation |
|---|---|
| **Backpressure** | Async-channel bounded queue; drop oldest on overflow (configurable) |
| **Retry Logic** | Exponential backoff (1s, 2s, 4s, 8s) to OTEL Collector |
| **Dead-Letter Queue** | Failed batches written to disk for manual replay |
| **Service Restart** | Windows Service Recovery policy (auto-restart) |
| **Graceful Shutdown** | Flush remaining queued logs before exit (30s timeout) |
| **Error Handling** | Malformed protocol messages logged but don't crash service |

### 9.3 Observability

| Aspect | Implementation |
|---|---|
| **Structured Logging** | tracing crate with spans for request flow |
| **Health Checks** | Windows Event Log entries for critical events |
| **Metrics** | Internal counters (logs_received, logs_exported, errors) exported via OTEL metrics |
| **Tracing** | Request IDs for end-to-end trace correlation |

### 9.4 Deployment

| Requirement | Implementation |
|---|---|
| **Platform** | Windows only (x86-64; ARM64 future consideration) |
| **Installation** | PowerShell script (Install-Service.ps1) + Registry entries |
| **Configuration** | TOML file in `%ProgramData%\log4TC\config\log4tc.toml` |
| **Log Storage** | Windows Event Log + local file (optional) |
| **Updates** | Executable replacement; service auto-restart on next poll |

---

## 10. Open Questions / Risks

### 10.1 Open Design Questions

1. **Distributed Tracing Integration**: Should Log4TC correlate its logs with trace spans from TwinCAT PLCs via trace context headers? Requires W3C Trace Context support in PLC library.

2. **Custom Attributes Extensibility**: Should users be able to define custom attribute mappings in config (e.g., `context.custom_field -> otel.attribute.name`)? Requires schema for mapping rules.

3. **Sampling Strategy**: If OTEL Collector applies sampling, should log4tc maintain per-task sample rates independently, or rely solely on collector-side sampling?

4. **Compression**: Should OTLP gRPC payloads be compressed by default? Trade-off: CPU (compression) vs bandwidth.

5. **Multi-Collector Failover**: Should the service support multiple OTEL Collector endpoints with automatic failover? Requires load balancing config.

### 10.2 Risk Mitigation

| Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|
| **OTEL Collector unavailable** | Medium | Logs queued locally; queue overflow = data loss | Dead-letter queue + retry backoff |
| **Protocol v1 version mismatch** | Low | PLC sends v2; service crashes on unknown version | Graceful error handling + version negotiation in PLC v0.3 |
| **Memory leak in batch accumulation** | Low | Growing RSS under sustained high load | Bounded queue + metrics monitoring |
| **Windows Service registration fails** | Low | Service won't start; manual registry cleanup required | Clear error messages + install script validation |
| **Hot-reload deadlock** | Medium | Config change hangs service | File-watcher timeout + graceful fallback to old config |
| **Timestamp precision loss** | Low | FILETIME -> Protobuf Timestamp (ns precision) | Document rounding behavior; use UTC everywhere |

### 10.3 Future Enhancements

- **Metrics Export**: Export internal metrics (logs_received, export_latency) as OTEL metrics
- **Trace Propagation**: Support W3C Trace Context for correlation with upstream traces
- **Plugin System**: Rust-based plugin architecture (wasm-based?) for custom transformations
- **Multi-Platform**: Support Linux systemd in addition to Windows Service
- **Kubernetes**: Native OTEL Operator integration for cloud deployments
- **UI Dashboard**: Web-based config/monitoring dashboard (future v2)

---

## Appendix A: Configuration Schema (log4tc.toml)

```toml
# Log4TC Service Configuration

[service]
name = "log4tc-service"
version = "1.0.0"
description = "TwinCAT to OpenTelemetry Bridge"

[ads]
port = 16150
buffer_size = 65536  # bytes

[dispatcher]
batch_size = 100
batch_timeout_secs = 5
queue_capacity = 10000
backpressure_strategy = "drop_oldest"  # or "block"

[otel]
endpoint = "http://localhost:4317"
timeout_secs = 10
retry_max_attempts = 3
compression = "gzip"  # or "none"

[otel.resource]
service_name = "log4tc"
service_version = "1.0.0"
service_namespace = "industrial"
deployment_environment = "production"

[logging]
level = "info"  # trace, debug, info, warn, error
file_path = "%ProgramData%\\log4TC\\logs\\service.log"
max_file_size = 10485760  # 10 MB
retention_days = 7
```

---

## Appendix B: Deployment Architecture

```
┌──────────────────────┐
│   Windows Server     │
│   (Target Host)      │
│                      │
│  ┌────────────────┐  │
│  │ log4tc-service │  │
│  │  (Rust binary) │  │
│  │   Port 16150   │  │
│  └────────┬───────┘  │
│           │ OTLP/gRPC│
│           │          │
└───────────┼──────────┘
            │
            │ 4317
            ▼
┌──────────────────────────────┐
│  OTEL Collector              │
│  (Docker / Kubernetes / VM)  │
│                              │
│  ┌──────────────────────┐   │
│  │ OTLP Receiver (4317) │   │
│  └──────────┬───────────┘   │
│             │                │
│  ┌──────────▼───────────┐   │
│  │ Processors           │   │
│  │ - Batch              │   │
│  │ - Attribute          │   │
│  │ - Sampler            │   │
│  └──────────┬───────────┘   │
│             │                │
│  ┌──────────▼────┬────┬──────────┐
│  │ Exporters      │    │          │
│  │                │    │          │
│  ▼                ▼    ▼          ▼
│ Datadog      New Relic Jaeger  Loki
│  OTLP        OTLP      OTLP     OTLP
│
└──────────────────────────────┘
```

---

**Document Version**: 1.0
**Last Updated**: 2026-03-31
**Author**: Log4TC Architecture Team

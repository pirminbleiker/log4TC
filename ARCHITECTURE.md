# Log4TC Rust Architecture

**Status**: Task #1 Complete  
**Last Updated**: March 31, 2026

---

## System Architecture

### High-Level Overview

```
┌──────────────────────────────────────────────────────────────┐
│                     TwinCAT PLC Layer                        │
│  (Beckhoff automation controller with IEC61131-3 runtime)   │
└────────────────┬──────────────────────────────────────────────┘
                 │
        ┌────────┴────────┐
        │                 │
        ▼                 ▼
   ADS Protocol      OTEL Protocol
   (Port 16150)      (Port 4318)
        │                 │
        └────────┬────────┘
                 │
┌────────────────▼──────────────────────────────────────────────┐
│                  Log4TC Service (Rust)                        │
│                                                               │
│  ┌──────────────────────────────────────────────────────┐   │
│  │              Protocol Receivers                       │   │
│  │  ┌──────────────────┐  ┌──────────────────────────┐  │   │
│  │  │  ADS Listener    │  │  OTEL HTTP Receiver      │  │   │
│  │  │  (TCP async)     │  │  (Axum HTTP framework)   │  │   │
│  │  └────────┬─────────┘  └────────────┬─────────────┘  │   │
│  │           │                        │                 │   │
│  │  ┌────────▼────────────────────────▼───────────┐    │   │
│  │  │       ADS Parser                            │    │   │
│  │  │  (Binary protocol parsing → LogEntry)       │    │   │
│  │  └────────┬─────────────────────────────────────┘    │   │
│  └──────────┼────────────────────────────────────────┘  │   │
│             │                                            │   │
│  ┌──────────▼────────────────────────────────────────┐  │   │
│  │      Async Channel (MPSC)                         │  │   │
│  │  Queue for log entries (configurable capacity)   │  │   │
│  └──────────┬────────────────────────────────────────┘  │   │
│             │                                            │   │
│  ┌──────────▼────────────────────────────────────────┐  │   │
│  │         Log Dispatcher                            │  │   │
│  │  - Routes to output plugins                       │  │   │
│  │  - Filters and enrichment                         │  │   │
│  │  - Error handling and retry logic                 │  │   │
│  └──────────┬────────────────────────────────────────┘  │   │
│             │                                            │   │
│  ┌──────────┴─────────────────────────────────────────┐ │   │
│  │          Output Plugins                            │ │   │
│  │  ┌────────┐ ┌────────┐ ┌──────┐ ┌──────────┐      │ │   │
│  │  │ NLog   │ │Graylog │ │InfluxDB │ SQL    │      │ │   │
│  │  └────────┘ └────────┘ └──────┘ └──────────┘      │ │   │
│  └──────────────────────────────────────────────────┘ │   │
└──────────────────────────────────────────────────────────┘
        │           │           │           │
        ▼           ▼           ▼           ▼
   NLog Server  Graylog Srv  InfluxDB   SQL Database
```

---

## Crate Architecture

### 1. log4tc-core
**Responsibility**: Core types, models, and configuration

```
log4tc-core/
├── models.rs
│   ├── LogLevel enum (Trace, Debug, Info, Warn, Error, Fatal)
│   │   └── OTEL severity mapping
│   ├── LogEntry struct
│   │   └── Full set of log metadata fields
│   └── LogRecord struct (OTEL representation)
│
├── formatter.rs
│   ├── MessageFormatter (template-based formatting)
│   └── Support for positional and named placeholders
│
├── config.rs
│   ├── AppSettings (root config)
│   ├── LoggingConfig (log format/level)
│   ├── ReceiverConfig (HTTP/gRPC listener)
│   ├── OutputConfig (plugin configuration)
│   └── ServiceConfig (runtime settings)
│
└── error.rs
    └── Core error types with error propagation
```

**Key Types**:
- `LogLevel` - Enum mapping to OTEL severity (0-5 → 1,5,9,13,17,21)
- `LogEntry` - Raw log entry from any source
- `LogRecord` - OTEL-formatted log record
- `AppSettings` - Complete application configuration
- `MessageFormatter` - Template string processing

**No External Dependencies**: Only serde, chrono, uuid, regex

---

### 2. log4tc-ads
**Responsibility**: ADS binary protocol support (legacy)

```
log4tc-ads/
├── protocol.rs
│   ├── AdsProtocolVersion enum
│   └── AdsLogEntry struct
│
├── parser.rs
│   ├── AdsParser (binary message parser)
│   └── BytesReader (little-endian binary reading)
│
├── listener.rs
│   ├── AdsListener (async TCP server)
│   └── Connection handler with ACK/NAK response
│
└── error.rs
    └── ADS-specific error types
```

**Protocol Details**:
- Version: 1 byte (currently 0x01)
- Strings: 2-byte length prefix + UTF-8 data
- Timestamps: 8 bytes FILETIME format (100-ns intervals since 1601)
- Values: Type-tagged (0=null, 1=i32, 2=f64, 3=string, 4=bool)

**TCP Server**:
- Listens on configurable host:port (default 127.0.0.1:16150)
- Accepts concurrent connections
- Per-connection async message handler
- ACK on success, error response on failure

---

### 3. log4tc-otel
**Responsibility**: OpenTelemetry protocol support (primary)

```
log4tc-otel/
├── receiver.rs
│   ├── OtelHttpReceiver (Axum HTTP server)
│   │   └── POST /v1/logs endpoint
│   └── OtelGrpcReceiver (tonic gRPC stub)
│
├── exporter.rs
│   ├── OtelExporter (HTTP client to collector)
│   ├── ExportConfig (batch size, retry config)
│   └── Retry logic with exponential backoff
│
├── mapping.rs
│   ├── OtelMapping (conversion utilities)
│   └── LogEntry → OTEL LogRecord conversion
│
└── error.rs
    └── OTEL-specific error types
```

**HTTP Receiver**:
- Listens on port 4318 (configurable)
- Accepts POST requests to `/v1/logs`
- CORS support built-in
- Type-safe request/response handling

**OTEL Exporter**:
- Batch processing (default 100 records/batch)
- Exponential backoff retry (100ms → 200ms → 400ms → 5s cap)
- HTTP/JSON serialization
- Proper OTEL LogsData format

**OTEL Mapping**:
```
LogEntry fields → OTEL Resource Attributes:
  project_name    → service.name
  app_name        → service.instance.id
  hostname        → host.name
  task_index      → process.pid
  task_name       → process.command_line

LogEntry fields → OTEL Scope Attributes:
  logger          → logger.name

LogEntry fields → OTEL Log Attributes:
  plc_timestamp   → plc.timestamp (ISO8601)
  task_cycle_counter → task.cycle
  online_change_count → online.changes
  source          → source.address
  arguments       → arg.0, arg.1, ...
  context         → context properties
```

---

### 4. log4tc-service
**Responsibility**: Service orchestration and execution

```
log4tc-service/
├── main.rs
│   ├── Application entry point
│   ├── Configuration loading
│   └── Tracing/logging initialization
│
├── service.rs
│   ├── Log4TcService (main orchestrator)
│   ├── Receiver startup (ADS + OTEL)
│   ├── Dispatcher initialization
│   └── Graceful shutdown handling
│
└── dispatcher.rs
    ├── LogDispatcher (router)
    ├── Output plugin management (TODO)
    └── Per-output routing logic (TODO)
```

**Service Lifecycle**:
1. Load configuration from JSON (or TOML)
2. Initialize tracing with environment filter
3. Create ADS listener (port 16150)
4. Create OTEL HTTP receiver (port 4318)
5. Create log dispatcher with output plugins
6. Start async tasks for receivers and dispatcher
7. Wait for shutdown signal (Ctrl-C)
8. Cancel tasks gracefully
9. Close all connections

**Configuration Example**:
```json
{
  "logging": {
    "logLevel": "info",
    "format": "json"
  },
  "receiver": {
    "host": "127.0.0.1",
    "httpPort": 4318,
    "grpcPort": 4317
  },
  "outputs": [
    {"Type": "nlog", "ConfigFile": "nlog.config"}
  ],
  "service": {
    "name": "Log4TcService",
    "channelCapacity": 10000,
    "shutdownTimeoutSecs": 30
  }
}
```

---

## Data Flow

### ADS Protocol Flow

```
TwinCAT PLC
    │ (ADS binary)
    ▼
AdsListener (TCP)
    │ (TCP stream)
    ▼
AdsParser.parse()
    │ (AdsLogEntry)
    ▼
Convert to LogEntry
    │
    ├─ Extract source IP
    ├─ Extract hostname
    └─ Copy all fields
    │
    ▼
Send ACK/NAK
    │
    ▼
Push to channel → Dispatcher
```

### OTEL Protocol Flow

```
TwinCAT or Client
    │ (HTTP POST JSON)
    ▼
OtelHttpReceiver (Axum)
    │ (HTTP body)
    ▼
Parse OTEL request (TODO)
    │
    ├─ Extract LogRecord
    └─ Convert to LogEntry
    │
    ▼
Send 200 OK
    │
    ▼
Push to channel → Dispatcher
```

### Output Flow

```
Dispatcher
    │ (LogEntry)
    ▼
Format message
    │ (MessageFormatter)
    ▼
Convert to OTEL LogRecord
    │ (LogRecord)
    ▼
Route to all outputs
    │
    ├─→ NLog plugin → NLog server
    ├─→ Graylog plugin → Graylog UDP
    ├─→ InfluxDB plugin → InfluxDB HTTP
    └─→ SQL plugin → Database
    │
    ▼
Export to collector (OTEL Exporter)
    │
    ├─ Batch if needed
    ├─ Retry with backoff
    └─ Log success/failure
```

---

## Error Handling Strategy

### By Crate

**log4tc-core**:
- Configuration parsing errors
- Invalid log level conversions
- Message formatting errors

**log4tc-ads**:
- TCP connection errors
- Protocol parsing errors (invalid version, incomplete messages)
- FILETIME conversion errors
- String encoding errors (non-UTF8)

**log4tc-otel**:
- HTTP request failures
- Serialization errors
- Export failures with automatic retry
- Invalid OTEL request format

**log4tc-service**:
- Service startup failures
- Channel overflow (when dispatcher lags)
- Output plugin failures
- Configuration loading errors

### Recovery Strategies

1. **ADS Parser**: Log warning, skip malformed message, send error ACK
2. **Output Failures**: Implement retry logic per plugin (TODO)
3. **Channel Full**: Drop oldest entry or apply backpressure (TODO)
4. **Service Startup**: Fail fast with clear error message
5. **Graceful Shutdown**: Wait for in-flight operations, force shutdown after timeout

---

## Concurrency Model

### Tokio-Based Async

```
Main thread (tokio runtime)
    │
    ├─→ Task: ADS listener (blocking accept loop)
    │   └─→ Per-connection handler (spawned task)
    │
    ├─→ Task: OTEL HTTP receiver (Axum server)
    │   └─→ Per-request handler (async)
    │
    ├─→ Task: Log dispatcher (channel consumer)
    │   └─→ Route to outputs (async)
    │
    └─→ Signal handler (Ctrl-C)
```

### Channel-Based Backpressure

```
Receivers → [MPSC Channel] → Dispatcher → Outputs
            Capacity: configurable (default 10000)
            
When full:
- Try_send returns error
- Receiver applies backpressure
- ADS: Returns error NAK to client
- OTEL: Returns 429 Too Many Requests
```

---

## Performance Characteristics

### Design Goals
- **Throughput**: 10,000+ logs/sec
- **Latency**: <2ms from receipt to dispatch
- **Memory**: <100MB base + minimal per-log overhead
- **CPU**: <5% under typical load

### Optimization Opportunities
1. **Batch export**: Currently per-log, can batch before export
2. **Connection pooling**: For database and HTTP outputs
3. **Message compression**: For network transport
4. **Streaming parsing**: For large messages
5. **Zero-copy where possible**: Use Bytes crate

---

## Security Considerations

### Current State
- No authentication on receivers
- No TLS/encryption
- No PII filtering
- No rate limiting

### Planned Enhancements
- HTTP Basic Auth / API Keys for OTEL receiver
- TLS/HTTPS support
- PII detection and scrubbing
- Rate limiting per source
- Audit logging of configuration changes

---

## Testing Strategy

### Unit Tests
- Type conversions and parsing
- Message formatting
- Configuration loading
- Error conditions

### Integration Tests
- End-to-end protocol handling
- Multiple concurrent connections
- Channel capacity and backpressure
- Graceful shutdown

### Performance Tests
- Throughput under load
- Latency distribution
- Memory profiling
- CPU usage patterns

---

## Deployment Architecture

### Single-Server Deployment

```
┌────────────────────────────────┐
│   Windows Server / Linux       │
│                                │
│  ┌──────────────────────────┐  │
│  │  log4tc-service (binary) │  │
│  │  - PID 1234              │  │
│  │  - Port 16150 (ADS)      │  │
│  │  - Port 4318 (OTEL)      │  │
│  └──────────────────────────┘  │
│                                │
│  config.json (configuration)   │
│  logs/ (output logs)           │
└────────────────────────────────┘
```

### Windows Service Deployment

```
Service Control Manager
    │
    ├─→ Service name: Log4TcService
    ├─→ Display name: Log4TC Logging Service
    ├─→ Executable: log4tc-service.exe
    ├─→ Arguments: (from config)
    └─→ Startup type: Automatic

    │
    ▼
Event Log
    └─→ Log4TC events (service start/stop/errors)
```

---

## Configuration Management

### Loading Hierarchy
1. Check environment variable `LOG4TC_CONFIG`
2. Load from specified path or default `config.json`
3. Parse JSON structure
4. Validate required fields
5. Apply defaults for optional fields

### Hot-Reload (Future)
- Watch config file for changes
- Reload on modification
- Signal output plugins to reconfigure
- No service restart required

---

## Integration Points

### Receiver Plugins (Can Add)
- gRPC receiver (framework in place)
- Kafka consumer (future)
- File tailer (future)
- Syslog receiver (future)

### Output Plugins (To Implement)
- NLog (HTTP target)
- Graylog (GELF/UDP)
- InfluxDB (HTTP/InfluxQL)
- SQL Server / PostgreSQL
- Elasticsearch
- Datadog
- New Relic
- Splunk

### OTEL Integration
- OpenTelemetry Collector (upstream)
- Observability backends (Jaeger, Prometheus, etc.)
- Cloud platforms (GCP Cloud Logging, AWS CloudWatch, Azure Monitor)

---

## Roadmap

### Phase 1: Foundation (✅ Complete)
- Workspace setup
- Core types and models
- ADS protocol parsing
- Message formatting
- Service orchestration

### Phase 2: Completion (Current)
- OTLP exporter protocol
- Output plugin system
- Dispatcher routing logic
- Unit test expansion

### Phase 3: Production
- Windows service integration
- Security hardening
- Performance optimization
- CI/CD pipeline

### Phase 4: Enhancement
- Additional output plugins
- gRPC receiver implementation
- Hot-reload configuration
- Distributed tracing

---

**Document Version**: 1.0  
**Rust Edition**: 2021  
**Minimum Rust Version**: 1.70+  
**Status**: Foundation Complete, Ready for Team Implementation

# Log4TC Rust Workspace

This document describes the Rust workspace structure for Log4TC, the logging bridge between Beckhoff TwinCAT PLCs and OpenTelemetry backends.

## Architecture Overview

```
TwinCAT PLC
    ↓ (OTEL Protocol on port 4318)
log4tc-service (HTTP/gRPC receiver)
    ├── log4tc-core (shared types)
    ├── log4tc-ads (legacy ADS protocol support)
    ├── log4tc-otel (OTEL receiver/exporter)
    └── dispatcher → outputs
```

## Workspace Structure

The workspace is organized as a monorepo with 4 crates:

### 1. `crates/log4tc-core` - Core Types and Models

**Purpose**: Defines fundamental data structures and types used across the system.

**Key Components**:
- `models.rs` - LogEntry, LogLevel, LogRecord
- `config.rs` - AppSettings, configuration structures
- `error.rs` - Error types

**Dependencies**: serde, chrono, uuid, bytes

**Key Types**:
```rust
pub struct LogEntry {
    pub message: String,
    pub logger: String,
    pub level: LogLevel,
    pub plc_timestamp: DateTime<Utc>,
    pub clock_timestamp: DateTime<Utc>,
    pub arguments: HashMap<usize, serde_json::Value>,
    pub context: HashMap<String, serde_json::Value>,
    // ... more fields
}

pub struct LogRecord {
    pub timestamp: DateTime<Utc>,
    pub body: serde_json::Value,
    pub severity_number: i32,
    pub resource_attributes: HashMap<String, serde_json::Value>,
    pub scope_attributes: HashMap<String, serde_json::Value>,
    pub log_attributes: HashMap<String, serde_json::Value>,
}
```

### 2. `crates/log4tc-ads` - ADS Protocol Support

**Purpose**: Parses the legacy ADS (Automation Device Specification) binary protocol from TwinCAT.

**Key Components**:
- `protocol.rs` - ADS protocol constants and structures
- `parser.rs` - Binary protocol parsing
- `error.rs` - ADS-specific error types

**Dependencies**: log4tc-core, tokio, bytes, chrono

**Key Types**:
```rust
pub struct AdsLogEntry {
    pub version: AdsProtocolVersion,
    pub message: String,
    pub logger: String,
    pub level: LogLevel,
    // ... plus timestamps, task info, args, context
}
```

**Note**: The ADS crate is included for potential backward compatibility or analysis of legacy protocol. The main receiver will use OTEL.

### 3. `crates/log4tc-otel` - OpenTelemetry Support

**Purpose**: Implements OTEL OTLP receiver endpoints and exporters.

**Key Components**:
- `receiver.rs` - HTTP and gRPC OTEL receivers
- `exporter.rs` - OTEL log record exporter with batching/retry
- `mapping.rs` - Conversion utilities between Log4TC and OTEL formats
- `error.rs` - OTEL-specific error types

**Dependencies**: log4tc-core, tokio, axum, tower, serde, prost, opentelemetry

**Key Types**:
```rust
pub struct OtelHttpReceiver {
    host: String,
    port: u16,
    log_tx: mpsc::Sender<LogEntry>,
}

pub struct OtelExporter {
    endpoint: String,
    batch_size: usize,
    max_retries: usize,
}
```

**Endpoints**:
- HTTP: `POST http://localhost:4318/v1/logs` (default, configurable)
- gRPC: `localhost:4317` (default, configurable)

### 4. `crates/log4tc-service` - Main Service

**Purpose**: Orchestrates the entire service - receivers, dispatcher, and outputs.

**Key Components**:
- `main.rs` - Application entry point, logging initialization
- `service.rs` - Main service orchestration
- `dispatcher.rs` - Routes logs to configured outputs

**Dependencies**: All other crates + tokio, axum, tracing

**Key Features**:
- Loads configuration from JSON
- Starts OTEL HTTP receiver
- Dispatches logs to configured output plugins
- Graceful shutdown on Ctrl-C

## Configuration

Configuration is loaded from `config.json` (or path in `LOG4TC_CONFIG` env var).

**Example structure**:
```json
{
  "logging": {
    "logLevel": "info",
    "format": "json",
    "outputPath": "logs/"
  },
  "receiver": {
    "host": "127.0.0.1",
    "httpPort": 4318,
    "grpcPort": 4317,
    "maxBodySize": 4194304,
    "requestTimeoutSecs": 30
  },
  "outputs": [
    {
      "Type": "nlog",
      "ConfigFile": "nlog.config"
    },
    {
      "Type": "graylog",
      "Host": "localhost",
      "Port": 12201
    }
  ],
  "service": {
    "name": "Log4TcService",
    "displayName": "Log4TC Logging Service",
    "channelCapacity": 10000,
    "shutdownTimeoutSecs": 30
  }
}
```

## Building

```bash
# Build all crates
cargo build

# Build specific crate
cargo build -p log4tc-service

# Release build
cargo build --release
```

## Testing

```bash
# Run all tests
cargo test

# Run tests for specific crate
cargo test -p log4tc-core

# Run with output
cargo test -- --nocapture
```

## Running

```bash
# Run with default config.json
cargo run -p log4tc-service

# Run with custom config
LOG4TC_CONFIG=my-config.json cargo run -p log4tc-service

# Run with debug logging
RUST_LOG=debug cargo run -p log4tc-service
```

## Dependency Graph

```
log4tc-service
├── log4tc-core (fundamental types)
├── log4tc-ads (legacy protocol)
├── log4tc-otel (OTEL support)
└── external: tokio, axum, serde, tracing, windows-service

log4tc-otel
├── log4tc-core
└── external: axum, tower, prost, opentelemetry

log4tc-ads
├── log4tc-core
└── external: tokio, bytes

log4tc-core
└── external: serde, chrono, uuid
```

## Development Workflow

### Adding a New Crate

```bash
# Create new crate (e.g., log4tc-nlog output)
cargo new crates/log4tc-nlog --lib

# Update workspace Cargo.toml to include new crate in [workspace] members
```

### Adding a Dependency

```bash
# Add to workspace
cd crates/log4tc-core
cargo add dependency_name
```

Or edit `Cargo.toml` directly and ensure workspace dependencies are used for shared deps.

## Key Design Decisions

1. **Workspace Structure**: Monorepo with clear separation of concerns across crates
2. **Async Runtime**: Tokio for async I/O operations
3. **Configuration**: JSON with serde deserialization, supports hot-reload (future)
4. **Error Handling**: Custom error types per crate using `thiserror` crate
5. **Logging**: Structured logging with `tracing` and `tracing-subscriber`
6. **Protocol**: OTEL HTTP/JSON (default), gRPC support (future)
7. **Windows Support**: Optional windows-rs for service integration

## Next Steps

1. **Task #2**: Implement core data model (additional validation, serialization)
2. **Task #3**: Implement configuration hot-reload
3. **Task #4**: Implement ADS TCP listener (if maintaining backward compatibility)
4. **Task #5**: Expand binary protocol parser for all 16 object types
5. **Task #6**: Implement message template formatter
6. **Task #7**: Finalize OTEL LogRecord mapping
7. **Task #8**: Implement OTLP exporter with batching/retry
8. **Task #9**: Implement async log dispatcher with backpressure

## References

- [OpenTelemetry Spec](https://opentelemetry.io/docs/specs/)
- [OTLP Protocol](https://github.com/open-telemetry/opentelemetry-proto)
- [Tokio Documentation](https://tokio.rs/)
- [Axum Documentation](https://docs.rs/axum/)
- [Serde Documentation](https://serde.rs/)

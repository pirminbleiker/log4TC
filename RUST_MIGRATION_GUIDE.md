# Rust Migration Quick Reference Guide

## Project Overview

**Goal**: Replace .NET service with Rust, switch from ADS to OpenTelemetry protocol

**Keep**: TwinCAT PLC library (currently working well)

---

## Current Architecture

```
TwinCAT PLC
    ↓ (ADS protocol on port 16150)
Log4TC Windows Service (.NET 6.0)
    ├── ADS Receiver (listens)
    ├── Log Dispatcher (routes)
    └── Output Plugins (NLog, Graylog, InfluxDB, SQL)
```

---

## Key Components to Replace (Rust)

### 1. Service Host
- **Current**: `Log4Tc.Service/Program.cs` (Windows service)
- **Replacement**: Rust HTTP/gRPC server (Windows service via windows-rs)
- **Files to replace**: 
  - `/d/Projects/Open Source/log4TC/source/Log4Tc/Log4Tc.Service/`

### 2. Protocol Receiver
- **Current**: `Log4Tc.Receiver/AdsLogReceiver.cs` (ADS on port 16150)
- **Replacement**: OpenTelemetry receiver (gRPC/HTTP)
- **New responsibility**: Accept OTEL signals instead of ADS
- **Files to replace**:
  - `/d/Projects/Open Source/log4TC/source/Log4Tc/Log4Tc.Receiver/`

### 3. Dispatcher/Router
- **Current**: `Log4Tc.Dispatcher/LogDispatcherService.cs` (routes to plugins)
- **Replacement**: Rust async router using tokio
- **Keep**: Same routing logic
- **Files to replace**:
  - `/d/Projects/Open Source/log4TC/source/Log4Tc/Log4Tc.Dispatcher/`

### 4. Data Models
- **Current**: `Log4Tc.Model/LogEntry.cs` (C# class)
- **Replacement**: Rust struct with serde serialization
- **Keep**: Same fields and behavior
- **Files to replace**:
  - `/d/Projects/Open Source/log4TC/source/Log4Tc/Log4Tc.Model/`

### 5. Output Plugins
- **Current**: .NET plugin system with ILogOutput interface
- **Replacement**: Rust trait-based system
- **Keep**: Same output destinations
- **Files to replace**:
  - `/d/Projects/Open Source/log4TC/source/Log4Tc/Log4Tc.Output*/`

### 6. Plugin Framework
- **Current**: `Log4Tc.Plugin/PluginLoader.cs` (reflection-based)
- **Replacement**: Rust trait objects or dynamic library loading
- **Files to replace**:
  - `/d/Projects/Open Source/log4TC/source/Log4Tc/Log4Tc.Plugin/`

---

## Keep As-Is

### TwinCAT Library
- **Location**: `/d/Projects/Open Source/log4TC/library/Log4TC.library`
- **Status**: Keep (no replacement)
- **Action Required**: 
  - Update to use OTEL instead of ADS
  - Modify to send telemetry data to Rust service's OTEL endpoint
  - Performance review for real-time constraints

### Configuration
- **Current**: JSON files (`appsettings.json`)
- **Recommendation**: Keep format, switch to Rust JSON parsing
- **Files**:
  - `/d/Projects/Open Source/log4TC/source/Log4Tc/Log4Tc.Service/appsettings*.json`

### Tests
- **Keep structure**: Same test scenarios
- **Replacement**: Rust test framework (cargo test)
- **Tools**: cargo test + additional Rust testing libraries

### CI/CD
- **Keep**: Azure Pipelines configuration
- **Update**: Build commands to use `cargo` instead of MSBuild

---

## Rust Crates to Consider

### Core
- **tokio** - Async runtime
- **axum** or **actix-web** - Web framework for HTTP/gRPC
- **tonic** - gRPC framework
- **serde** + **serde_json** - Serialization/configuration
- **tracing** - Logging/observability

### Protocol
- **opentelemetry** - OTEL SDK
- **opentelemetry-proto** - OTEL protocol definitions

### Output Integrations
- **reqwest** - HTTP client (for API-based outputs)
- **sqlx** - SQL database access
- **nlog-rs** or http client - NLog integration
- **graylog** or **gelf** - Graylog integration
- **influxdb_client** - InfluxDB integration

### Windows/Service
- **windows-rs** - Windows API access
- **winservice** or **windows-service** - Service hosting

### Testing
- **tokio::test** - Async test macro
- **mockall** - Mocking framework
- **testcontainers** - Integration test containers

---

## Migration Path

### Phase 1: Foundation
1. Set up Rust project structure with cargo
2. Create core data models (LogEntry, LogLevel)
3. Implement configuration parsing from JSON
4. Create basic async HTTP/gRPC listener

### Phase 2: Core Service
1. Implement OpenTelemetry receiver
2. Create dispatcher/router logic
3. Set up Windows service wrapper
4. Test with mock data

### Phase 3: Outputs
1. Implement plugin trait system
2. Create adapters for each output (NLog, Graylog, InfluxDB, SQL)
3. Test each output independently

### Phase 4: TwinCAT Integration
1. Update TwinCAT library to use OTEL instead of ADS
2. Update PLC code to send OTEL telemetry
3. Integration testing with real PLC

### Phase 5: Deployment
1. Update CI/CD to build Rust service
2. Create Windows installer for Rust binary
3. Migration guide for existing users
4. Deprecation of .NET service

---

## Critical Protocol Details to Preserve

### Current Binary Protocol (ADS)
**File**: `/d/Projects/Open Source/log4TC/source/Log4Tc/Log4Tc.Receiver/AdsLogReceiver.cs`

**Structure**:
```
[Version: 1 byte]
[Message: string]
[Logger: string]
[Level: byte]
[PlcTimestamp: FILETIME]
[ClockTimestamp: FILETIME]
[TaskIndex: int32]
[TaskName: string]
[TaskCycleCounter: uint32]
[AppName: string]
[ProjectName: string]
[OnlineChangeCount: uint32]
[Arguments: { [Type: byte][ArgIndex: byte][Value: object] }*]
[Context: { [Type: byte][Scope: byte][Name: string][Value: object] }*]
```

**Must Preserve**:
- All metadata fields
- Message template support (MessageTemplates.org)
- Argument indexing system
- Context property scoping
- Binary encoding efficiency

### OTEL Mapping Strategy
Consider how each ADS field maps to OpenTelemetry semantics:
- Message → Span.Name or LogRecord.Body
- Logger → Instrumentation Scope
- Level → Severity Level
- Timestamps → Span.Start/End Time
- Task* fields → Resource attributes
- Arguments/Context → Span attributes

---

## Configuration Migration

### Current Format
```json
{
  "Logging": { "LogLevel": { "Default": "Information" } },
  "Outputs": [
    { "Type": "nlog" },
    { "Type": "graylog", "Host": "localhost", "Port": 12201 }
  ]
}
```

### Rust Equivalent
Use same JSON structure, parse with serde:
```rust
use serde::{Deserialize};

#[derive(Deserialize)]
struct AppSettings {
    logging: LoggingConfig,
    outputs: Vec<OutputConfig>,
}

#[derive(Deserialize)]
struct OutputConfig {
    #[serde(rename = "Type")]
    output_type: String,
    #[serde(flatten)]
    settings: serde_json::Value,
}
```

---

## Testing Strategy

### Unit Tests
- LogEntry model parsing
- Message template formatting
- Configuration loading
- Each output adapter

### Integration Tests
- End-to-end with mock OTEL client
- Output plugin chaining
- Configuration hot-reload
- Error handling and recovery

### Performance Tests
- Throughput (logs/second)
- Latency from receipt to output
- Memory usage
- CPU utilization

---

## Known Challenges

1. **Binary Protocol Decoding**: OTEL changes communication model significantly
2. **Plugin System**: Rust's trait system works differently than .NET interfaces
3. **Dynamic Configuration**: Rust compile-time traits vs .NET runtime plugins
4. **Windows Service**: Less mature than .NET ecosystem, requires windows-rs
5. **Performance**: Real-time PLC → non-real-time service timing constraints

---

## File Mapping for Replacement

| .NET Component | Path | → | Rust Crate/Module |
|---|---|---|---|
| Log4Tc.Service | `/source/Log4Tc/Log4Tc.Service/` | → | `main.rs` / Service module |
| Log4Tc.Receiver | `/source/Log4Tc/Log4Tc.Receiver/` | → | `otel_receiver.rs` |
| Log4Tc.Dispatcher | `/source/Log4Tc/Log4Tc.Dispatcher/` | → | `dispatcher.rs` |
| Log4Tc.Model | `/source/Log4Tc/Log4Tc.Model/` | → | `models.rs` |
| Log4Tc.Output | `/source/Log4Tc/Log4Tc.Output/` | → | `outputs/mod.rs` |
| Log4Tc.Plugin | `/source/Log4Tc/Log4Tc.Plugin/` | → | `plugin_loader.rs` |
| appsettings.json | `/source/Log4Tc/Log4Tc.Service/` | → | `config.rs` |
| Log4Tc.Setup | `/source/Log4Tc/Log4Tc.Setup/` | → | `msi-builder` crate or NSIS |

---

## Useful Documentation References

- **OpenTelemetry Spec**: https://opentelemetry.io/docs/specs/
- **OTLP Protocol**: https://github.com/open-telemetry/opentelemetry-specification/blob/main/specification/protocol/otlp.md
- **Message Templates**: https://messagetemplates.org/
- **Tokio Documentation**: https://tokio.rs/
- **Serde Documentation**: https://serde.rs/


# OTEL Mapping & Export Specification - Log4TC

**Document Version**: 1.0  
**OTEL Spec Version**: 1.0  
**Last Updated**: March 31, 2026

---

## Overview

This document specifies how Log4TC translates log entries from both ADS and OTEL protocols into the OpenTelemetry Logs Data Model, and how the Rust service exports them to OTEL collectors.

---

## OTEL Logs Data Model

### Components

```
LogRecord
├── Timestamp (nanoseconds since Unix epoch)
├── ObservedTimestamp (when observed by service)
├── Severity (number and text)
├── SeverityText ("TRACE", "DEBUG", etc.)
├── Body (log message string)
├── Attributes (key-value pairs)
├── Resource (service/host/environment info)
├── InstrumentationScope (logger/library info)
└── Flags & TraceContext (for tracing correlation)
```

---

## Field Mapping

### LogEntry → OTEL LogRecord

#### Resource Attributes

These identify the **service/host** producing the logs.

```
Log4TC Field          OTEL Resource Attribute      Type        Notes
─────────────────────────────────────────────────────────────────
project_name          service.name                  string      Project name in TwinCAT
app_name              service.instance.id           string      Application name
hostname              host.name                     string      PLC host
task_index            process.pid                   int         Task/process ID
(constant)            service.namespace             string      "log4tc" (fixed)
(constant)            telemetry.sdk.name            string      "log4tc" (fixed)
(constant)            telemetry.sdk.language        string      "rust" (fixed)
(constant)            telemetry.sdk.version         string      (package version)
```

**Example Resource**:
```json
{
  "service.name": "MotorController",
  "service.instance.id": "MainApp",
  "host.name": "plc-01.factory.local",
  "process.pid": 1,
  "service.namespace": "log4tc",
  "telemetry.sdk.name": "log4tc",
  "telemetry.sdk.language": "rust",
  "telemetry.sdk.version": "1.0.0"
}
```

#### Scope Attributes

These identify the **logger/library** producing logs.

```
Log4TC Field          OTEL Scope Attribute         Type        Notes
─────────────────────────────────────────────────────────────────
logger                logger.name                   string      Logger component name
(constant)            code.namespace                string      "log4tc" (fixed)
```

**Example Scope**:
```json
{
  "logger.name": "Hardware.Motors.Controller",
  "code.namespace": "log4tc"
}
```

#### Log Record Body and Severity

```
Log4TC Field          OTEL Field                    Type        Notes
─────────────────────────────────────────────────────────────────
message               body                          string      Formatted log message
level                 severity_number               int         0→1, 1→5, 2→9, 3→13, 4→17, 5→21
level                 severity_text                 string      "TRACE", "DEBUG", "INFO", etc.
```

**Severity Mapping**:
```
Log4TC Level  →  OTEL Severity#  →  OTEL Severity Text
─────────────────────────────────────────────────────
Trace (0)     →  1               →  "TRACE"
Debug (1)     →  5               →  "DEBUG"
Info (2)      →  9               →  "INFO"
Warn (3)      →  13              →  "WARN"
Error (4)     →  17              →  "ERROR"
Fatal (5)     →  21              →  "FATAL"
```

#### Log Record Attributes

These are **custom attributes** specific to each log entry.

```
Log4TC Field              OTEL Log Attribute           Type        Notes
──────────────────────────────────────────────────────────────────
plc_timestamp             plc.timestamp                string      ISO8601 format
clock_timestamp           clock.timestamp              string      ISO8601 format
task_name                 process.command_line         string      Task name
task_cycle_counter        task.cycle                   int         Cycle counter
online_change_count       online.changes               int         Deployment count
(source IP:Port)          source.address               string      TCP/UDP source
(none)                    service.version              string      (if available)

arguments[0]              arg.0                        any         Message argument #0
arguments[1]              arg.1                        any         Message argument #1
arguments[n]              arg.n                        any         Message argument #n

context[key]              context.key                  any         Custom context variable
```

**Example Log Attributes**:
```json
{
  "plc.timestamp": "2026-03-31T10:30:45.123456Z",
  "clock.timestamp": "2026-03-31T10:30:45.234567Z",
  "process.command_line": "MainTask",
  "task.cycle": 1000,
  "online.changes": 0,
  "source.address": "192.168.1.100:54321",
  "arg.0": 42,
  "arg.1": "motor_speed",
  "context.user": "operator1",
  "context.shift": "morning"
}
```

---

## Complete Mapping Example

### Input: Log4TC LogEntry

```rust
LogEntry {
    version: 1,
    message: "Motor speed exceeded: {0} RPM, threshold: {1}",
    logger: "Hardware.Motors.SpeedMonitor",
    level: Warn,
    plc_timestamp: DateTime<2026-03-31T10:30:45.123456Z>,
    clock_timestamp: DateTime<2026-03-31T10:30:45.234567Z>,
    task_index: 1,
    task_name: "MainTask",
    task_cycle_counter: 5000,
    app_name: "FactoryController",
    project_name: "AutomationSystem",
    hostname: "plc-01.factory.local",
    online_change_count: 2,
    source: "192.168.1.100:54321",
    arguments: {0: 3500, 1: 3000},
    context: {"user": "operator1", "shift": "morning"}
}
```

### Output: OTEL LogRecord (JSON)

```json
{
  "resourceLogs": [
    {
      "resource": {
        "attributes": [
          {"key": "service.name", "value": {"stringValue": "AutomationSystem"}},
          {"key": "service.instance.id", "value": {"stringValue": "FactoryController"}},
          {"key": "host.name", "value": {"stringValue": "plc-01.factory.local"}},
          {"key": "process.pid", "value": {"intValue": "1"}},
          {"key": "service.namespace", "value": {"stringValue": "log4tc"}},
          {"key": "telemetry.sdk.name", "value": {"stringValue": "log4tc"}},
          {"key": "telemetry.sdk.language", "value": {"stringValue": "rust"}}
        ]
      },
      "scopeLogs": [
        {
          "scope": {
            "name": "Hardware.Motors.SpeedMonitor",
            "attributes": [
              {"key": "code.namespace", "value": {"stringValue": "log4tc"}}
            ]
          },
          "logRecords": [
            {
              "timeUnixNano": "1743380445123456000",
              "observedTimeUnixNano": "1743380445234567000",
              "severityNumber": 13,
              "severityText": "WARN",
              "body": {
                "stringValue": "Motor speed exceeded: 3500 RPM, threshold: 3000"
              },
              "attributes": [
                {"key": "plc.timestamp", "value": {"stringValue": "2026-03-31T10:30:45.123456Z"}},
                {"key": "clock.timestamp", "value": {"stringValue": "2026-03-31T10:30:45.234567Z"}},
                {"key": "process.command_line", "value": {"stringValue": "MainTask"}},
                {"key": "task.cycle", "value": {"intValue": "5000"}},
                {"key": "online.changes", "value": {"intValue": "2"}},
                {"key": "source.address", "value": {"stringValue": "192.168.1.100:54321"}},
                {"key": "arg.0", "value": {"intValue": "3500"}},
                {"key": "arg.1", "value": {"intValue": "3000"}},
                {"key": "context.user", "value": {"stringValue": "operator1"}},
                {"key": "context.shift", "value": {"stringValue": "morning"}}
              ]
            }
          ]
        }
      ]
    }
  ]
}
```

---

## Export Configuration

### OTLP Export Protocol

#### Option A: HTTP/JSON (Recommended)

```
Protocol: HTTP POST
Endpoint: https://collector.example.com:4318/v1/logs
Content-Type: application/json
Headers:
  Authorization: Bearer <token>
  User-Agent: log4tc/1.0

Advantages:
  - Human-readable payloads
  - Easy to debug
  - Works with most collectors

Disadvantages:
  - Larger payload size (~2-3x vs protobuf)
  - Slightly higher CPU (JSON encoding)
```

**Request Example**:
```http
POST /v1/logs HTTP/1.1
Host: collector.example.com:4318
Content-Type: application/json
Content-Length: 1250
Authorization: Bearer token123

{
  "resourceLogs": [...]
}
```

#### Option B: HTTP/Protobuf

```
Protocol: HTTP POST
Endpoint: https://collector.example.com:4318/v1/logs
Content-Type: application/x-protobuf
Headers:
  Authorization: Bearer <token>

Advantages:
  - Compact binary format
  - Lower bandwidth
  - Standard OTEL specification

Disadvantages:
  - Not human-readable
  - Requires protobuf library
```

#### Option C: gRPC

```
Protocol: gRPC (HTTP/2 multiplexed)
Endpoint: collector.example.com:4317
TLS: Required (default)

Advantages:
  - Bidirectional streaming
  - Connection reuse
  - Native flow control

Disadvantages:
  - More complex implementation
  - Requires gRPC library and HTTP/2
```

### Batching

```
Default batch size: 100 logs
Max queue before batch: 50 logs
Flush timeout: 5 seconds (if batch not full)

Logic:
1. Accumulate LogRecords in buffer
2. When buffer reaches batch_size → export immediately
3. If no new logs for flush_timeout → export partial batch
4. Prevents: memory growth, stale logs
```

**Example**:
```
Timeline:
T=0.0s   LogRecord #1 arrives (buffer_size=1)
T=0.5s   LogRecord #50 arrives (buffer_size=50)
T=1.0s   LogRecord #100 arrives (buffer_size=100)
         → Export batch immediately (100 logs)
         → Reset buffer
T=1.5s   LogRecord #101 arrives (buffer_size=1)
T=6.5s   5 second timeout reached, only 1 log in buffer
         → Export partial batch (1 log)
```

### Retry Policy

```
Retry Strategy: Exponential backoff with cap

Attempt  Wait Time  Cumulative Time
────────────────────────────────
1        100ms      100ms
2        200ms      300ms
3        400ms      700ms
4        800ms      1500ms
5        1600ms     3100ms (capped at 5s from here)
6        5s         8100ms
7        5s         13100ms
8        5s         18100ms
Max retries: 8 (total max wait: ~30 seconds before giving up)

On final failure:
- Log error with details
- Drop batch (prevent memory growth)
- Continue processing new logs
```

**Code Logic**:
```rust
async fn export_with_retry(batch: Vec<LogRecord>) -> Result<()> {
    let mut wait_ms = 100;
    let max_wait_ms = 5000;
    
    for attempt in 1..=8 {
        match self.client.post(&self.endpoint)
            .json(&batch)
            .send()
            .await
        {
            Ok(resp) if resp.status().is_success() => return Ok(()),
            Ok(resp) if resp.status() == 429 => {
                // Rate limited - back off
                tokio::time::sleep(Duration::from_millis(wait_ms)).await;
            }
            Err(e) if e.is_timeout() || e.is_connect() => {
                // Network error - back off
                tokio::time::sleep(Duration::from_millis(wait_ms)).await;
            }
            Err(e) => {
                // Fatal error
                return Err(e.into());
            }
        }
        
        wait_ms = std::cmp::min(wait_ms * 2, max_wait_ms);
    }
    
    // Exhausted retries
    eprintln!("Failed to export {} logs after 8 attempts", batch.len());
    Ok(()) // Don't fail service, just log
}
```

---

## TLS/HTTPS Configuration

### Enforcement

```rust
// In exporter setup:
let client = reqwest::Client::builder()
    .https_only(true)  // Reject non-HTTPS endpoints
    .timeout(Duration::from_secs(30))
    .build()?;
```

### Certificate Validation

```
By default:
- System root CA store is used
- Server certificate is validated
- Hostname verification is performed
- Self-signed certificates are rejected

To allow self-signed (development only):
export OTEL_EXPORTER_OTLP_INSECURE=false
// Still requires proper hostname in cert
```

### Custom Certificates

```rust
// Load custom CA certificate
let ca_cert = reqwest::Certificate::from_pem(&ca_pem)?;

let client = reqwest::Client::builder()
    .add_root_certificate(ca_cert)
    .https_only(true)
    .build()?;
```

---

## Environment Variables

### OTEL Standard Variables

```bash
# Collector endpoint (required)
export OTEL_EXPORTER_OTLP_ENDPOINT=https://collector.example.com:4318

# Headers (authentication, custom headers)
export OTEL_EXPORTER_OTLP_HEADERS=Authorization=Bearer%20token,CustomHeader=value

# Protocol (http/protobuf or grpc)
export OTEL_EXPORTER_OTLP_PROTOCOL=http/protobuf

# Timeout (seconds)
export OTEL_EXPORTER_OTLP_TIMEOUT=30

# TLS certificate file
export OTEL_EXPORTER_OTLP_CERTIFICATE=/path/to/ca-bundle.crt

# Insecure mode (development only, false by default)
export OTEL_EXPORTER_OTLP_INSECURE=false
```

### Log4TC Specific Variables

```bash
# Configuration file
export LOG4TC_CONFIG=/etc/log4tc/config.toml

# Batch size for export
export LOG4TC_BATCH_SIZE=100

# Max retries on export failure
export LOG4TC_MAX_RETRIES=8

# Flush timeout (seconds)
export LOG4TC_FLUSH_TIMEOUT=5

# Log level for service
export RUST_LOG=info,log4tc=debug
```

### Example Setup

```bash
#!/bin/bash

# Jaeger collector (local development)
export OTEL_EXPORTER_OTLP_ENDPOINT=http://localhost:4318
export OTEL_EXPORTER_OTLP_INSECURE=true

# Or: Grafana Cloud (production)
export OTEL_EXPORTER_OTLP_ENDPOINT=https://logs-prod-us-central1.grafana.net:443/loki/api/v1/otlp
export OTEL_EXPORTER_OTLP_HEADERS="Authorization=Bearer grafana-api-token"

# Or: Google Cloud Logging
export OTEL_EXPORTER_OTLP_ENDPOINT=https://opentelemetry.googleapis.com:443

# Or: Datadog
export OTEL_EXPORTER_OTLP_ENDPOINT=https://http-intake.logs.datadoghq.com:443
export OTEL_EXPORTER_OTLP_HEADERS="DD-API-KEY=datadog-api-key"

# Common settings
export LOG4TC_BATCH_SIZE=100
export LOG4TC_FLUSH_TIMEOUT=5
export RUST_LOG=info
```

---

## Semantic Conventions

### Service

```
service.name                string    Name of the service
service.namespace           string    Namespace of the service
service.instance.id         string    Instance ID of the service
service.version             string    Version of the service
```

### Host

```
host.name                   string    Hostname
host.id                     string    Unique host identifier
host.type                   string    Type of host (physical, vm, etc)
```

### Process

```
process.pid                 int       Process ID
process.executable.name     string    Executable name
process.command_line        string    Full command line
process.parent_pid          int       Parent process ID
```

### Source

```
source.address              string    Source IP or hostname
source.port                 int       Source port number
```

### Custom Log4TC Attributes

```
plc.timestamp               string    Timestamp from PLC (ISO8601)
task.cycle                  int       Task cycle counter
online.changes              int       Online change count
arg.n                       any       Message template argument
context.*                   any       Custom context variable
```

---

## Compatibility

### Collectors Tested

- **Jaeger** (OpenTelemetry Reference Implementation)
- **Grafana Cloud** (SaaS observability)
- **Google Cloud Logging** (via OTLP receiver)
- **Datadog** (via OTLP receiver)
- **Dynatrace** (via OTLP receiver)
- **New Relic** (via OTLP receiver)
- **Splunk** (via OTLP receiver)

### Backend Support

From OTEL collector, logs can be routed to:
- Datadog
- Elasticsearch/OpenSearch
- Grafana Loki
- Google Cloud Logging
- Splunk
- Dynatrace
- AWS CloudWatch
- Azure Monitor
- Prometheus (via prometheus-remote-write exporter)

---

## Troubleshooting

### Check Connectivity

```bash
# Test OTLP endpoint
curl -v -X POST https://collector:4318/v1/logs \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer token" \
  -d '{"resourceLogs":[]}'

# Should return 200 OK (even with empty logs)
```

### Enable Debug Logging

```bash
export RUST_LOG=trace,log4tc=trace

# This will show:
# - Each log entry received
# - Serialization details
# - HTTP requests/responses
# - Retry attempts
```

### Verify Exporter Configuration

```bash
# Check environment variables
env | grep -i otel

# Check config file
cat /etc/log4tc/config.toml | grep -A5 otel
```

### Common Issues

1. **"HTTPS only" error**
   - Cause: Endpoint is http://, not https://
   - Fix: Use https:// URL or set OTEL_EXPORTER_OTLP_INSECURE=false

2. **"Certificate validation failed"**
   - Cause: Self-signed cert or custom CA
   - Fix: Provide CA cert via OTEL_EXPORTER_OTLP_CERTIFICATE

3. **"429 Too Many Requests"**
   - Cause: Rate limited by collector
   - Fix: Reduce batch size, increase flush timeout, or increase quota

4. **"Connection timeout"**
   - Cause: Collector unreachable or slow
   - Fix: Check network, verify endpoint, increase timeout

---

## Performance

### Throughput

```
At 10,000 logs/sec with 100-log batches:
- 100 batches/sec exported
- ~1MB/sec with JSON encoding
- ~300KB/sec with protobuf encoding
- Network: ~10Mbps (JSON) or ~3Mbps (protobuf)
```

### Latency

```
From LogEntry arrival to OTEL export:
- Parsing:        <1ms
- Conversion:     <1ms
- Batching wait:  0-5s (configurable)
- HTTP POST:      10-100ms (depending on network)

Total (95th percentile): ~50-100ms
Total (99th percentile): ~150-300ms
```

### Memory

```
Batching buffer (100 logs):
- Each LogRecord: ~2-3KB (JSON), ~500B (protobuf)
- 100 logs: 200-300KB buffered
- Minimal heap impact
```

---

**Document Status**: Complete  
**OTEL Spec Version**: 1.0.2  
**Last Review**: March 31, 2026

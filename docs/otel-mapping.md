# OpenTelemetry (OTEL) Mapping Specification for log4TC

## 1. Overview

log4TC is migrating from multiple custom output formats (Graylog GELF, InfluxDB, etc.) to a single, standardized [OpenTelemetry (OTEL)](https://opentelemetry.io/) output protocol. This transition provides significant benefits:

### Why OpenTelemetry?

- **Vendor-agnostic standard**: OTEL is a Cloud Native Computing Foundation (CNCF) standard, ensuring long-term stability and ecosystem support
- **Single protocol for all backends**: Replace multiple plugin-based outputs with one unified OTLP (OpenTelemetry Protocol) implementation
- **Ecosystem flexibility**: Route logs to any OTEL-compatible backend (Loki, Elasticsearch, Jaeger, Datadog, New Relic, etc.) via an OTEL Collector
- **Rich semantic conventions**: Built-in support for structured logging, trace correlation, and common resource attributes
- **Production-ready**: Wide adoption in industry with excellent tooling and documentation
- **Simplified maintenance**: Focus on a single, well-defined protocol rather than maintaining multiple output plugins

### Architecture

```
TwinCAT PLC
    ↓
ADS Binary Protocol
    ↓
Rust Service (log4TC)
    ↓
OpenTelemetry Protocol (gRPC or HTTP)
    ↓
OTEL Collector
    ↓
Backends (Loki, Elasticsearch, Jaeger, etc.)
```

## 2. OpenTelemetry Logs Data Model Reference

The OpenTelemetry Logs data model defines a `LogRecord` structure. Understanding this structure is essential for correct mapping:

### LogRecord Structure

| Field | Type | Description |
|-------|------|-------------|
| `TimeUnixNano` | `uint64` | Log record timestamp as Unix time in nanoseconds |
| `SeverityNumber` | `int32` | Numeric severity level (0=UNSPECIFIED, 1=TRACE, 2=TRACE2, 3=TRACE3, 4=TRACE4, 5=DEBUG, 6=DEBUG2, 7=DEBUG3, 8=DEBUG4, 9=INFO, 10=INFO2, 11=INFO3, 12=INFO4, 13=WARN, 14=WARN2, 15=WARN3, 16=WARN4, 17=ERROR, 18=ERROR2, 19=ERROR3, 20=ERROR4, 21=FATAL, 22=FATAL2, 23=FATAL3, 24=FATAL4) |
| `SeverityText` | `string` | Text representation of severity (e.g., "INFO", "ERROR") |
| `Body` | `AnyValue` | The log message body (typically a string) |
| `Attributes` | `map<string, AnyValue>` | Custom key-value pairs providing context |
| `Resource` | `Resource` | Identifies the source (service, host, etc.) |
| `InstrumentationScope` | `InstrumentationScope` | Identifies the component that produced the log |
| `TraceId` | `bytes` | Optional trace ID for correlation with traces |
| `SpanId` | `bytes` | Optional span ID for correlation with spans |
| `Flags` | `uint32` | Optional trace flags (e.g., sampled flag) |
| `DroppedAttributesCount` | `uint32` | Count of attributes not included due to limits |

### Resource Object

The `Resource` object describes the entity producing telemetry:

| Field | Type | Description |
|-------|------|-------------|
| `Attributes` | `map<string, AnyValue>` | Resource attributes (service.name, host.name, etc.) |
| `DroppedAttributesCount` | `uint32` | Count of attributes not included due to limits |

### AnyValue Type

OTEL uses a flexible `AnyValue` type that can represent:
- `StringValue`: UTF-8 string
- `BoolValue`: Boolean
- `IntValue`: 64-bit signed integer
- `DoubleValue`: 64-bit floating-point
- `ArrayValue`: Array of AnyValue
- `KeyValueListValue`: Map of string keys to AnyValue

## 3. LogEntry → OTEL LogRecord Mapping

The log4TC `LogEntry` class is the primary data structure for logs. The following table defines how each field maps to OpenTelemetry:

### Mapping Table

| LogEntry Field | Target | OTEL Field | Type | Notes |
|---|---|---|---|---|
| `Timestamp` | LogRecord | `TimeUnixNano` | uint64 | Convert `PlcTimestamp` to Unix nanoseconds. Primary timestamp for the log record. |
| `Level` | LogRecord | `SeverityNumber` + `SeverityText` | int32 + string | Map according to OTEL severity table below. |
| `FormattedMessage` | LogRecord | `Body` | AnyValue (StringValue) | Preferred over raw Message; includes argument substitution. |
| `Message` | Attributes | `log4tc.message_template` | string | Raw message template without argument substitution. |
| `Source` | Attributes + Resource | `log4tc.source` (Attribute) + Resource | string | Source identifies the log producer. Include in both attributes and as a resource attribute. |
| `Logger` | Attributes | `log4tc.logger` | string | Logger name or identifier. |
| `Hostname` | Resource | `host.name` | string | Hostname of the system. |
| `TaskName` | Attributes + Resource | `log4tc.task_name` (Attribute) + Resource | string | TwinCAT task name. Include in both. |
| `TaskIndex` | Attributes + Resource | `log4tc.task_index` (Attribute) + Resource | int64 | Task index for identification. |
| `TaskCycleCounter` | Attributes | `log4tc.task_cycle_counter` | int64 | Task cycle counter. |
| `AppName` | Resource | `plc.app_name` | string | PLC application name. |
| `ProjectName` | Resource | `plc.project_name` | string | TwinCAT project name. |
| `PlcTimestamp` | LogRecord | `TimeUnixNano` | uint64 | Primary log record timestamp. |
| `ClockTimestamp` | Attributes | `log4tc.clock_timestamp_iso8601` | string | System clock timestamp in ISO 8601 format for reference. |
| `OnlineChangeCount` | Attributes | `log4tc.online_change_count` | int64 | Online change counter. |
| `Arguments[*]` | Attributes | `log4tc.arg_<name>` | AnyValue | Each message argument is mapped with `log4tc.arg_` prefix. Type is determined from the argument's runtime type. |
| `Context[*]` | Attributes | `log4tc.<key>` | AnyValue | Each context property is included as an attribute with `log4tc.` prefix. |

### Log Level Mapping

The following table maps log4TC `LogLevel` to OTEL severity:

| log4TC LogLevel | OTEL SeverityNumber | OTEL SeverityText | Rationale |
|---|---|---|---|
| `Trace` | 1 | `TRACE` | Low-level diagnostic information |
| `Debug` | 5 | `DEBUG` | Diagnostic information useful for developers |
| `Info` | 9 | `INFO` | General informational message |
| `Warn` | 13 | `WARN` | Warning condition that should be investigated |
| `Error` | 17 | `ERROR` | Error condition indicating failure |
| `Fatal` | 21 | `FATAL` | Critical error that may cause shutdown |

## 4. Resource Attributes

The Resource object identifies the log source and must include the following attributes:

### Required Resource Attributes

| Attribute | Value | Description |
|---|---|---|
| `service.name` | `"log4tc"` | Fixed service identifier |
| `service.version` | `"<version>"` | log4TC service version (e.g., "2.0.0") |
| `host.name` | `<Hostname>` | From `LogEntry.Hostname` |

### Recommended Resource Attributes

| Attribute | Value | Source | Description |
|---|---|---|---|
| `plc.task_name` | `<TaskName>` | From `LogEntry.TaskName` | TwinCAT task name |
| `plc.task_index` | `<TaskIndex>` | From `LogEntry.TaskIndex` | Task index |
| `plc.app_name` | `<AppName>` | From `LogEntry.AppName` | PLC application name |
| `plc.project_name` | `<ProjectName>` | From `LogEntry.ProjectName` | TwinCAT project name |
| `service.instance.id` | `<Source>` | From `LogEntry.Source` | Unique service instance identifier |

### Example Resource

```json
{
  "attributes": {
    "service.name": "log4tc",
    "service.version": "2.0.0",
    "host.name": "plc-host-01",
    "plc.app_name": "MyApplication",
    "plc.project_name": "TwinCAT_Project",
    "plc.task_name": "PlcTask",
    "plc.task_index": 1,
    "service.instance.id": "192.168.1.100"
  }
}
```

## 5. OTLP Export Configuration

### Protocol Selection

log4TC supports two OTLP protocols:

#### gRPC (Recommended for Production)

- **Endpoint**: `grpc://localhost:4317` (default OTEL Collector)
- **Protocol**: HTTP/2 with Protocol Buffers
- **Compression**: gzip (recommended)
- **Connection**: Persistent, multiplexed
- **Bandwidth**: More efficient than HTTP

```toml
[otlp]
protocol = "grpc"
endpoint = "localhost:4317"
compression = "gzip"
timeout_ms = 10000
headers = { "Authorization" = "Bearer <token>" }
```

#### HTTP/Protobuf

- **Endpoint**: `http://localhost:4318/v1/logs`
- **Protocol**: HTTP/1.1 with Protocol Buffers
- **Compression**: gzip
- **Connection**: Connection pooling recommended
- **Bandwidth**: Less efficient than gRPC

```toml
[otlp]
protocol = "http"
endpoint = "http://localhost:4318"
content_encoding = "gzip"
timeout_ms = 10000
headers = { "Authorization" = "Bearer <token>" }
```

### Connection Configuration

| Setting | Type | Default | Description |
|---|---|---|---|
| `endpoint` | string | `localhost:4317` | OTEL Collector endpoint |
| `protocol` | enum | `grpc` | Transport protocol (grpc or http) |
| `timeout_ms` | int | 10000 | Request timeout in milliseconds |
| `compression` | string | `gzip` | Compression algorithm (none, gzip, zstd) |
| `headers` | map | {} | Custom HTTP headers for authentication |
| `certificate_path` | string | None | Path to TLS certificate for gRPC |
| `insecure` | bool | false | Allow insecure TLS (dev only) |

### Example Configuration

```toml
[otlp]
protocol = "grpc"
endpoint = "localhost:4317"
compression = "gzip"
timeout_ms = 10000
insecure = false

# Optional headers for authentication
[otlp.headers]
Authorization = "Bearer eyJhbGc..."
X-Custom-Header = "value"
```

## 6. Batching & Retry

log4TC uses a `BatchLogRecordProcessor` to optimize throughput and reduce network overhead.

### Batch Configuration

| Setting | Type | Default | Description |
|---|---|---|---|
| `batch_size` | int | 512 | Maximum number of LogRecords in a batch |
| `max_queue_size` | int | 2048 | Maximum number of LogRecords queued |
| `scheduled_delay_millis` | int | 5000 | Delay before flushing a partial batch (5 seconds) |
| `export_timeout_millis` | int | 30000 | Timeout for a single export request (30 seconds) |

### Retry Strategy

Retry uses exponential backoff with jitter:

| Setting | Type | Default | Description |
|---|---|---|---|
| `initial_interval_millis` | int | 100 | Initial retry delay |
| `max_interval_millis` | int | 10000 | Maximum retry delay (10 seconds) |
| `multiplier` | float | 1.5 | Exponential backoff multiplier |
| `max_attempts` | int | 5 | Maximum retry attempts before dropping |

### Retry Algorithm

```
delay = initial_interval_millis
for attempt in 0..max_attempts:
    try export()
    if success:
        return
    delay = min(delay * multiplier, max_interval_millis)
    delay += random(0, delay * 0.1)  # jitter
    sleep(delay)
drop_batch()
```

### Configuration Example

```toml
[otlp.batch]
batch_size = 512
max_queue_size = 2048
scheduled_delay_millis = 5000
export_timeout_millis = 30000

[otlp.retry]
initial_interval_millis = 100
max_interval_millis = 10000
multiplier = 1.5
max_attempts = 5
```

## 7. Semantic Conventions

log4TC adheres to the following OTEL semantic conventions:

### Log Attributes

| Attribute | Description | Example |
|---|---|---|
| `log.level` | Deprecated; use SeverityText instead | "INFO" |
| `log.record.uid` | Unique log record identifier | UUID or sequence |
| `log.record.original` | Original unformatted log message | Message template |

### Host Attributes

| Attribute | Description | Example |
|---|---|---|
| `host.name` | Hostname | "plc-01" |
| `host.id` | Unique host identifier | AMS Net ID |
| `host.ip` | Host IP address | "192.168.1.100" |
| `host.arch` | CPU architecture | "x86_64" |

### Service Attributes

| Attribute | Description | Example |
|---|---|---|
| `service.name` | Service name | "log4tc" |
| `service.version` | Service version | "2.0.0" |
| `service.instance.id` | Service instance | "192.168.1.100" |
| `service.namespace` | Service namespace | "log4tc-prod" |

### Process Attributes

| Attribute | Description | Example |
|---|---|---|
| `process.pid` | Process ID | 1234 |
| `process.executable.name` | Executable name | "log4tc" |

### Custom log4TC Attributes

All log4TC-specific attributes use the `log4tc.` prefix:

| Attribute | Description | Type |
|---|---|---|
| `log4tc.source` | Log source identifier | string |
| `log4tc.logger` | Logger name | string |
| `log4tc.message_template` | Unformatted message | string |
| `log4tc.task_name` | TwinCAT task name | string |
| `log4tc.task_index` | Task index | int64 |
| `log4tc.task_cycle_counter` | Task cycle counter | int64 |
| `log4tc.clock_timestamp_iso8601` | System clock time | string |
| `log4tc.online_change_count` | Online change counter | int64 |
| `log4tc.arg_<name>` | Message argument | AnyValue |

## 8. Example OTLP Payloads

### Example 1: Simple Info Log

```json
{
  "resourceLogs": [
    {
      "resource": {
        "attributes": [
          { "key": "service.name", "value": { "stringValue": "log4tc" } },
          { "key": "service.version", "value": { "stringValue": "2.0.0" } },
          { "key": "host.name", "value": { "stringValue": "plc-01" } },
          { "key": "plc.project_name", "value": { "stringValue": "MyProject" } }
        ]
      },
      "scopeLogs": [
        {
          "scope": {
            "name": "log4tc",
            "version": "2.0.0"
          },
          "logRecords": [
            {
              "timeUnixNano": "1648742400000000000",
              "severityNumber": 9,
              "severityText": "INFO",
              "body": { "stringValue": "Task cycle started" },
              "attributes": [
                { "key": "log4tc.source", "value": { "stringValue": "192.168.1.100" } },
                { "key": "log4tc.logger", "value": { "stringValue": "PlcTask" } },
                { "key": "log4tc.task_name", "value": { "stringValue": "PlcTask" } },
                { "key": "log4tc.task_index", "value": { "intValue": "1" } },
                { "key": "log4tc.task_cycle_counter", "value": { "intValue": "42" } }
              ]
            }
          ]
        }
      ]
    }
  ]
}
```

### Example 2: Error Log with Arguments

```json
{
  "resourceLogs": [
    {
      "resource": {
        "attributes": [
          { "key": "service.name", "value": { "stringValue": "log4tc" } },
          { "key": "service.version", "value": { "stringValue": "2.0.0" } },
          { "key": "host.name", "value": { "stringValue": "plc-01" } },
          { "key": "plc.app_name", "value": { "stringValue": "MotionControl" } }
        ]
      },
      "scopeLogs": [
        {
          "scope": {
            "name": "log4tc"
          },
          "logRecords": [
            {
              "timeUnixNano": "1648742401000000000",
              "severityNumber": 17,
              "severityText": "ERROR",
              "body": { "stringValue": "Motor 1 error: Position mismatch 45.5 degrees, tolerance 2.0" },
              "attributes": [
                { "key": "log4tc.source", "value": { "stringValue": "192.168.1.100" } },
                { "key": "log4tc.logger", "value": { "stringValue": "MotorControl" } },
                { "key": "log4tc.task_name", "value": { "stringValue": "MotionTask" } },
                { "key": "log4tc.message_template", "value": { "stringValue": "Motor {} error: {} {}, tolerance {}" } },
                { "key": "log4tc.arg_motor_id", "value": { "intValue": "1" } },
                { "key": "log4tc.arg_error", "value": { "stringValue": "Position mismatch" } },
                { "key": "log4tc.arg_value", "value": { "doubleValue": 45.5 } },
                { "key": "log4tc.arg_tolerance", "value": { "doubleValue": 2.0 } }
              ]
            }
          ]
        }
      ]
    }
  ]
}
```

### Example 3: Batch Export (Multiple Logs)

```json
{
  "resourceLogs": [
    {
      "resource": {
        "attributes": [
          { "key": "service.name", "value": { "stringValue": "log4tc" } },
          { "key": "service.version", "value": { "stringValue": "2.0.0" } },
          { "key": "host.name", "value": { "stringValue": "plc-01" } }
        ]
      },
      "scopeLogs": [
        {
          "scope": {
            "name": "log4tc",
            "version": "2.0.0"
          },
          "logRecords": [
            {
              "timeUnixNano": "1648742400000000000",
              "severityNumber": 5,
              "severityText": "DEBUG",
              "body": { "stringValue": "Debug message 1" },
              "attributes": [
                { "key": "log4tc.source", "value": { "stringValue": "192.168.1.100" } }
              ]
            },
            {
              "timeUnixNano": "1648742401000000000",
              "severityNumber": 9,
              "severityText": "INFO",
              "body": { "stringValue": "Info message 2" },
              "attributes": [
                { "key": "log4tc.source", "value": { "stringValue": "192.168.1.100" } }
              ]
            },
            {
              "timeUnixNano": "1648742402000000000",
              "severityNumber": 13,
              "severityText": "WARN",
              "body": { "stringValue": "Warning message 3" },
              "attributes": [
                { "key": "log4tc.source", "value": { "stringValue": "192.168.1.100" } }
              ]
            }
          ]
        }
      ]
    }
  ]
}
```

## 9. OTEL Collector Configuration

The OTEL Collector is a standalone service that receives logs from log4TC and routes them to backends. The following examples demonstrate common setups.

### Basic Receiver Configuration

```yaml
receivers:
  otlp:
    protocols:
      grpc:
        endpoint: 0.0.0.0:4317
      http:
        endpoint: 0.0.0.0:4318
```

### Example 1: Grafana Loki Backend

Loki stores logs efficiently and integrates with Grafana for visualization.

```yaml
receivers:
  otlp:
    protocols:
      grpc:
        endpoint: 0.0.0.0:4317

processors:
  batch:
    send_batch_size: 100
    timeout: 10s
  
  # Optional: filter/transform logs
  attributes:
    actions:
      - key: environment
        value: production
        action: insert

exporters:
  loki:
    endpoint: "http://loki:3100/loki/api/v1/push"
    # Tenant ID (if using multi-tenancy)
    tenant_id: "default"

service:
  pipelines:
    logs:
      receivers: [otlp]
      processors: [batch, attributes]
      exporters: [loki]
```

### Example 2: Elasticsearch Backend

Elasticsearch provides full-text search and analytics on logs.

```yaml
receivers:
  otlp:
    protocols:
      grpc:
        endpoint: 0.0.0.0:4317

processors:
  batch:
    send_batch_size: 100
    timeout: 10s

exporters:
  elasticsearch:
    endpoints: ["http://elasticsearch:9200"]
    auth:
      authenticator: basicauth/elastic
    pipeline: es_pipeline
    logs_index: "otel-logs-%{+yyyy.MM.dd}"

extensions:
  basicauth/elastic:
    client_auth:
      username: elastic
      password: ${ELASTICSEARCH_PASSWORD}

service:
  extensions: [basicauth/elastic]
  pipelines:
    logs:
      receivers: [otlp]
      processors: [batch]
      exporters: [elasticsearch]
```

### Example 3: Jaeger Backend

Jaeger supports trace correlation for logs.

```yaml
receivers:
  otlp:
    protocols:
      grpc:
        endpoint: 0.0.0.0:4317
      http:
        endpoint: 0.0.0.0:4318

processors:
  batch:
    send_batch_size: 100
    timeout: 10s
  
  # Add resource detection (AWS, GCP, etc.)
  resource/detect:
    detectors: [system]

exporters:
  jaeger:
    endpoint: http://jaeger:14250
    tls:
      insecure: true

service:
  pipelines:
    logs:
      receivers: [otlp]
      processors: [batch, resource/detect]
      exporters: [jaeger]
```

### Example 4: Multi-Backend Setup

Route logs to multiple backends simultaneously.

```yaml
receivers:
  otlp:
    protocols:
      grpc:
        endpoint: 0.0.0.0:4317

processors:
  batch:
    send_batch_size: 100
    timeout: 10s
  
  # Enrich logs with environment info
  resource:
    attributes:
      - key: environment
        value: production
        action: insert

exporters:
  loki:
    endpoint: "http://loki:3100/loki/api/v1/push"
  elasticsearch:
    endpoints: ["http://elasticsearch:9200"]
    logs_index: "otel-logs-%{+yyyy.MM.dd}"
  logging:  # For debugging
    loglevel: debug

service:
  pipelines:
    logs:
      receivers: [otlp]
      processors: [batch, resource]
      exporters: [loki, elasticsearch, logging]
```

## 10. Performance Considerations

### Attribute Cardinality

High cardinality attributes (many unique values) can significantly impact performance and storage costs. Guidelines:

- **Low cardinality** (< 1000 unique values): Safe for all backends. Examples: `service.name`, `host.name`, `log4tc.task_name`
- **Medium cardinality** (1000-100k unique values): Use with caution. Examples: `log4tc.source`, user IDs
- **High cardinality** (> 100k unique values): Avoid or aggregate. Examples: Request IDs, timestamps with millisecond precision

### Recommendations

1. **Avoid dynamic attributes**: Do not create attributes from unbounded data (e.g., user input, request parameters with many variations)
2. **Pre-aggregate arguments**: If arguments contain high-cardinality values, consider pre-processing or sampling
3. **Use Resource attributes sparingly**: Resource attributes apply to all logs, multiplying their cardinality impact
4. **Set reasonable batch sizes**: Larger batches (512-1024) reduce network overhead but increase memory usage

### Attribute Value Size Limits

Most backends have limits on attribute value sizes:

| Backend | Limit | Recommendation |
|---|---|---|
| Loki | No explicit limit, but long strings impact performance | Max 4KB per attribute |
| Elasticsearch | 256KB per field | Max 8KB per attribute |
| Datadog | 128KB per attribute | Max 4KB per attribute |
| Generic | Varies | Max 1KB per attribute |

### Recommendations

- **Truncate large values**: Log messages are typically limited to 4KB
- **Avoid nested objects**: Use flat attribute names with dots (e.g., `context.user.id`)
- **Disable verbose arguments**: In production, consider filtering or sampling verbose argument logs

### Memory Management

The BatchLogRecordProcessor queues logs in memory:

- **Queue size**: `max_queue_size = 2048` (default) allows up to 2048 pending logs
- **Memory per log**: Typical log record ~500 bytes + attributes
- **Max memory**: 2048 × 500 bytes ≈ 1 MB (plus OS overhead)

### Adjustments for High-Volume Scenarios

For high-volume logging (> 10,000 logs/sec):

1. Increase batch size: `batch_size = 2048`
2. Increase queue size: `max_queue_size = 8192`
3. Reduce scheduled delay: `scheduled_delay_millis = 2000`
4. Use gRPC with compression for lower bandwidth
5. Consider sampling for lower-criticality logs

### Sampling Configuration

Implement sampling to reduce volume for development/testing:

```toml
[otlp.sampling]
enabled = true
sampling_rate = 0.1  # 10% of logs
rules = [
  { level = "ERROR", sampling_rate = 1.0 },      # Always sample errors
  { level = "WARN", sampling_rate = 0.5 },       # Sample 50% of warnings
  { level = "INFO", sampling_rate = 0.1 },       # Sample 10% of info logs
  { level = "DEBUG", sampling_rate = 0.01 },     # Sample 1% of debug logs
]
```

### Summary

| Scenario | Recommendation |
|---|---|
| Development | High sampling rate, no compression, HTTP protocol |
| Production | Full logs, gRPC + gzip, max batch size |
| High-volume | Sampling + batching, tune queue size |
| Low-bandwidth | gRPC + gzip, smaller batches |
| Low-latency | Smaller batches, shorter scheduled_delay |


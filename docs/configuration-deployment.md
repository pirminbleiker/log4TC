# Log4TC Rust Service - Configuration & Deployment Guide

## Table of Contents
1. [Overview](#overview)
2. [System Requirements](#system-requirements)
3. [Configuration File Format](#configuration-file-format)
4. [Configuration Reference](#configuration-reference)
5. [Migration from .NET](#migration-from-net)
6. [Installation](#installation)
7. [Windows Service Management](#windows-service-management)
8. [Docker Deployment](#docker-deployment)
9. [Monitoring & Health Checks](#monitoring--health-checks)
10. [Troubleshooting](#troubleshooting)
11. [Example Configurations](#example-configurations)

---

## Overview

Log4TC Rust Service is a high-performance, native logging bridge that receives telemetry from Beckhoff TwinCAT PLCs via OpenTelemetry (OTLP) and exports logs to various backends.

### Architecture

```
┌─────────────────────┐
│   TwinCAT PLC       │
│  (Sends OTEL data)  │
└──────────┬──────────┘
           │ OTLP gRPC/HTTP
           ▼
┌─────────────────────────────────────┐
│ Log4TC Rust Service                 │
│ ┌──────────────────────────────────┐│
│ │ ADS Listener (Port 16150)        ││
│ │ (Legacy binary protocol)         ││
│ └──────────────────────────────────┘│
│ ┌──────────────────────────────────┐│
│ │ OTLP Receiver (Port 4317/4318)   ││
│ │ (gRPC/HTTP)                      ││
│ └──────────────────────────────────┘│
│ ┌──────────────────────────────────┐│
│ │ Log Dispatcher & Exporters       ││
│ ├──────────────────────────────────┤│
│ │ • OpenTelemetry Exporter         ││
│ │ • Log Processors (batching, retry)││
│ └──────────────────────────────────┘│
└─────────────────────────────────────┘
           │
    ┌──────┴──────┬──────────┬─────────┐
    ▼             ▼          ▼         ▼
[OTEL Coll]  [Grafana]  [Prometheus] [Loki]
```

### Key Features

- **Native Rust**: High performance, minimal resource usage
- **OpenTelemetry Compatible**: Standard telemetry protocol
- **Windows Service**: Runs as native Windows Service
- **Multiple Exporters**: Support for various backends
- **Hot Reload**: Configuration changes without restart (optional)
- **Health Checks**: Built-in monitoring endpoints
- **Metrics**: Prometheus-compatible metrics endpoint

### System Requirements

**Minimum**:
- Windows 10 / Windows Server 2016+
- x86-64 processor
- 512 MB RAM (minimum)
- .NET Runtime 6.0 or later (for legacy ADS support)

**Recommended**:
- Windows Server 2019+
- 2 GB RAM
- SSD for log storage
- Static IP address

---

## Configuration File Format

The service uses **TOML** configuration format (`config.toml`). TOML provides:
- Clear, readable syntax
- Native support for tables (sections)
- Type safety (numbers, booleans, strings)
- Comments for documentation
- Better tooling than JSON for Rust

### Location

Default config locations (checked in order):
1. Environment variable: `LOG4TC_CONFIG`
2. Current working directory: `./config.toml`
3. Service directory: `{InstallDir}/config.toml`
4. Windows config directory: `%ProgramData%/log4tc/config.toml`

### File Format Example

```toml
# Log4TC Configuration
# Service configuration in TOML format

[service]
name = "log4tc"
log_level = "info"
worker_threads = 4

[ads]
# Legacy ADS binary protocol receiver
port = 16150
bind_address = "0.0.0.0"
max_connections = 100
buffer_size = 65536

[otel]
# OpenTelemetry OTLP receiver
endpoint = "0.0.0.0:4317"
protocol = "grpc"  # or "http"
compression = "gzip"
timeout_seconds = 30

[logging]
level = "info"
format = "json"
output = "file"
```

---

## Configuration Reference

### [service] Section

Core service settings.

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `name` | string | `"log4tc"` | Service identifier, used in logs and metrics |
| `log_level` | string | `"info"` | Log level: `trace`, `debug`, `info`, `warn`, `error` |
| `worker_threads` | integer | `4` | Number of worker threads (tokio runtime) |
| `graceful_shutdown_timeout` | integer | `30` | Shutdown timeout in seconds |

**Example**:
```toml
[service]
name = "log4tc-prod"
log_level = "info"
worker_threads = 8
graceful_shutdown_timeout = 30
```

### [ads] Section

Beckhoff ADS binary protocol receiver (legacy support).

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `enabled` | boolean | `true` | Enable ADS receiver |
| `port` | integer | `16150` | ADS listening port |
| `bind_address` | string | `"0.0.0.0"` | Bind address (use `"127.0.0.1"` for local only) |
| `max_connections` | integer | `100` | Maximum concurrent ADS connections |
| `buffer_size` | integer | `65536` | Input buffer size in bytes (64 KB default) |
| `max_message_size` | integer | `1048576` | Max single message size (1 MB) |

**Example**:
```toml
[ads]
enabled = true
port = 16150
bind_address = "0.0.0.0"
max_connections = 100
buffer_size = 65536
max_message_size = 1048576
```

### [otel] Section

OpenTelemetry OTLP receiver configuration.

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `enabled` | boolean | `false` | Enable OTEL receiver |
| `endpoint` | string | `"0.0.0.0:4317"` | Listen address and port |
| `protocol` | string | `"grpc"` | Protocol: `"grpc"` (4317) or `"http"` (4318) |
| `compression` | string | `"gzip"` | Compression: `"gzip"`, `"none"` |
| `timeout_seconds` | integer | `30` | Request timeout in seconds |
| `max_request_body_size` | integer | `4194304` | Max request size (4 MB) |

**Example**:
```toml
[otel]
enabled = true
endpoint = "0.0.0.0:4317"
protocol = "grpc"
compression = "gzip"
timeout_seconds = 30
max_request_body_size = 4194304
```

### [otel.tls] Section (Optional)

TLS configuration for OTEL receiver.

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `enabled` | boolean | `false` | Enable TLS |
| `cert_path` | string | - | Path to TLS certificate file |
| `key_path` | string | - | Path to TLS private key file |
| `client_auth_required` | boolean | `false` | Require client certificate |
| `ca_path` | string | - | CA certificate for client verification |

**Example**:
```toml
[otel.tls]
enabled = true
cert_path = "C:\\certs\\server.crt"
key_path = "C:\\certs\\server.key"
client_auth_required = false
```

### [otel.headers] Section (Optional)

Custom headers to expect from OTEL clients.

```toml
[otel.headers]
"Authorization" = "Bearer token123"
"X-Custom-Header" = "value"
```

### [otel.batch] Section

Log exporter batching configuration.

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `enabled` | boolean | `true` | Enable batch processing |
| `max_batch_size` | integer | `512` | Max logs per batch |
| `scheduled_delay_ms` | integer | `5000` | Time to wait before sending batch (ms) |
| `max_queue_size` | integer | `2048` | Max queued logs before dropping |
| `max_export_batch_size` | integer | `512` | Max logs in single export call |
| `export_timeout_ms` | integer | `30000` | Export operation timeout (ms) |

**Example**:
```toml
[otel.batch]
enabled = true
max_batch_size = 512
scheduled_delay_ms = 5000
max_queue_size = 2048
max_export_batch_size = 512
export_timeout_ms = 30000
```

### [otel.retry] Section

Automatic retry configuration for failed exports.

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `enabled` | boolean | `true` | Enable automatic retry |
| `initial_interval_ms` | integer | `5000` | Initial retry delay (ms) |
| `max_interval_ms` | integer | `30000` | Maximum retry delay (ms) |
| `max_elapsed_time_ms` | integer | `300000` | Max total retry time (5 min) |
| `multiplier` | float | `1.5` | Exponential backoff multiplier |

**Example**:
```toml
[otel.retry]
enabled = true
initial_interval_ms = 5000
max_interval_ms = 30000
max_elapsed_time_ms = 300000
multiplier = 1.5
```

### [otel.resource] Section

Resource attributes describing the service (OpenTelemetry semantics).

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `service.name` | string | `"log4tc"` | Service name |
| `service.version` | string | `"1.0.0"` | Service version |
| `service.namespace` | string | - | Service namespace/team |
| `host.name` | string | (auto-detect) | Hostname |

**Custom Attributes**:
```toml
[otel.resource]
"service.name" = "log4tc"
"service.version" = "1.0.0"
"deployment.environment" = "production"
"service.instance.id" = "log4tc-01"
```

### [logging] Section

Internal service logging configuration (for log4tc's own logs).

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `level` | string | `"info"` | Log level: `trace`, `debug`, `info`, `warn`, `error` |
| `format` | string | `"json"` | Format: `"json"` or `"text"` |
| `output` | string | `"file"` | Output: `"file"`, `"console"`, `"both"` |
| `directory` | string | `"%ProgramData%/log4tc/logs"` | Log directory |
| `max_file_size` | integer | `104857600` | Max log file size (100 MB) |
| `max_backup_files` | integer | `10` | Number of backup files to keep |
| `include_target` | boolean | `true` | Include log target (module) |
| `include_thread_id` | boolean | `true` | Include thread ID |

**Example**:
```toml
[logging]
level = "info"
format = "json"
output = "both"
directory = "%ProgramData%/log4tc/logs"
max_file_size = 104857600
max_backup_files = 10
```

### [exporters] Section (Optional)

Export destinations for logs. Each exporter is a table with type and configuration.

#### Exporter Types

**OpenTelemetry Collector**:
```toml
[[exporters]]
type = "otlp"
endpoint = "http://localhost:4317"
protocol = "grpc"
timeout_seconds = 30
```

**Grafana Loki**:
```toml
[[exporters]]
type = "loki"
endpoint = "http://localhost:3100"
tenant_id = "default"
batch_size = 1000
```

**Prometheus Remote Write** (for metrics):
```toml
[[exporters]]
type = "prometheus"
endpoint = "http://localhost:9009/api/v1/write"
```

**NLog HTTP Endpoint**:
```toml
[[exporters]]
type = "nlog"
endpoint = "http://localhost:8080/api/logs"
```

**InfluxDB**:
```toml
[[exporters]]
type = "influxdb"
url = "http://localhost:8086"
database = "log4tc"
retention_policy = "autogen"
```

**SQL Database**:
```toml
[[exporters]]
type = "sql"
connection_string = "Server=localhost;Database=log4tc;User Id=sa;Password=..."
batch_size = 100
```

---

## Migration from .NET

### Configuration Mapping

The old .NET `appsettings.json` format needs to be converted to TOML. Here's the mapping:

#### Old .NET Format
```json
{
  "Logging": {
    "LogLevel": {
      "Default": "Information",
      "Microsoft": "Warning",
      "Microsoft.Hosting.Lifetime": "Information"
    }
  },
  "Outputs": [
    {
      "Type": "nlog"
    },
    {
      "Type": "graylog",
      "Host": "localhost",
      "Port": 12201
    },
    {
      "Type": "influxdb",
      "Config": {
        "Url": "http://localhost:8086",
        "Database": "log4tc"
      }
    }
  ]
}
```

#### New Rust TOML Format
```toml
[service]
name = "log4tc"
log_level = "info"
worker_threads = 4

[ads]
enabled = true
port = 16150
bind_address = "0.0.0.0"
max_connections = 100

[otel]
enabled = false
endpoint = "0.0.0.0:4317"
protocol = "grpc"

[logging]
level = "info"
format = "json"
output = "file"
directory = "%ProgramData%/log4tc/logs"

[[exporters]]
type = "otlp"
endpoint = "http://localhost:4317"
protocol = "grpc"

[[exporters]]
type = "graylog"
endpoint = "localhost:12201"

[[exporters]]
type = "influxdb"
url = "http://localhost:8086"
database = "log4tc"
```

### What Changed

**Removed** (these output plugins are deprecated):
- NLog HTTP integration → Use OTLP directly
- Direct InfluxDB line protocol → Use OTLP or use exporter
- SQL direct outputs → Use OTLP or configure exporter
- Graylog GELF integration → Use OTLP or exporter

**Reason**: The new architecture standardizes on OpenTelemetry. All backends are integrated via OTLP and exporters, providing a cleaner, more maintainable architecture.

### Migration Script (PowerShell)

```powershell
# migration-config.ps1
# Converts appsettings.json to config.toml

param(
    [string]$InputFile = "appsettings.json",
    [string]$OutputFile = "config.toml"
)

$json = Get-Content $InputFile | ConvertFrom-Json

$toml = @"
# Auto-generated from appsettings.json
# Please review and adjust values as needed

[service]
name = "log4tc"
log_level = "$(($json.Logging.LogLevel.Default -replace 'Information', 'info' -replace 'Warning', 'warn' -replace 'Debug', 'debug').ToLower())"
worker_threads = 4

[ads]
enabled = true
port = 16150
bind_address = "0.0.0.0"
max_connections = 100

[logging]
level = "info"
format = "json"
output = "file"
directory = "%ProgramData%/log4tc/logs"
"@

# Add exporters from Outputs array
$json.Outputs | ForEach-Object {
    $type = $_.Type
    $toml += "`n`n[[exporters]]`ntype = `"$type`""
    
    # Add type-specific configuration
    switch ($type) {
        "nlog" {
            $toml += "`nendpoint = `"http://localhost:8080/api/logs`""
        }
        "graylog" {
            $toml += "`nendpoint = `"$($_.Host):$($_.Port)`""
        }
        "influxdb" {
            $toml += "`nurl = `"$($_.Config.Url)`""
            $toml += "`ndatabase = `"$($_.Config.Database)`""
        }
    }
}

$toml | Out-File -FilePath $OutputFile -Encoding UTF8
Write-Host "Converted $InputFile to $OutputFile"
Write-Host "Please review the generated file and adjust values as needed."
```

**Usage**:
```powershell
.\migration-config.ps1 -InputFile appsettings.json -OutputFile config.toml
```

---

## Installation

### Prerequisites

- Windows 10 or later / Windows Server 2016+
- Administrator privileges
- TCP ports available: 16150 (ADS), 4317/4318 (OTEL)

### Option 1: Windows Service Executable

The simplest deployment method. The executable installs/manages itself as a Windows Service.

**Installation**:
```cmd
log4tc-service.exe install
```

**Verify**:
```cmd
sc query log4tc
```

**Output**:
```
SERVICE_NAME: log4tc
        TYPE               : 10  WIN32_OWN_PROCESS
        STATE              : 1   STOPPED
        WIN32_EXIT_CODE    : 0   (0x0)
        SERVICE_EXIT_CODE  : 0   (0x0)
        CHECKPOINT         : 0x0
        WAIT_HINT          : 0x0
```

**Configuration**:
1. Create config directory: `mkdir C:\ProgramData\log4tc`
2. Copy `config.toml` to `C:\ProgramData\log4tc\config.toml`
3. Create logs directory: `mkdir C:\ProgramData\log4tc\logs`

**Start the service**:
```cmd
net start log4tc
```

### Option 2: Manual Installation with Custom Service Name

```cmd
# Install with custom service name and display name
sc create log4tc-prod ^
  binPath= "C:\Program Files\log4tc\log4tc-service.exe" ^
  DisplayName= "Log4TC Production Service" ^
  start= auto

# Set description
sc description log4tc-prod "Log4TC Production telemetry service"

# Start the service
net start log4tc-prod
```

### Option 3: Windows Installer (MSI)

A WiX-based installer is available. This provides:
- Visual installation wizard
- Automatic registry entries
- Shortcuts in Start Menu
- Uninstall support
- Automatic config directory setup

**Installation**:
```cmd
msiexec /i log4tc-1.0.0.msi /quiet /norestart
```

**With UI**:
```cmd
msiexec /i log4tc-1.0.0.msi
```

**Uninstall**:
```cmd
msiexec /x log4tc-1.0.0.msi /quiet
```

### Directory Structure

After installation, your directory structure should look like:

```
C:\Program Files\log4tc\
├── log4tc-service.exe          # Main service executable
├── log4tc-service.exe.config   # .NET runtime config (if needed)
└── README.txt                  # Installation notes

C:\ProgramData\log4tc\
├── config.toml                 # Configuration file
├── logs\
│   ├── log4tc.log              # Current log file (JSON)
│   ├── log4tc.log.1            # Rotated log files
│   └── log4tc.log.2
└── cache\                       # Temporary files (optional)
    └── state.dat               # Service state persistence
```

---

## Windows Service Management

### Start/Stop/Restart

**Start the service**:
```cmd
net start log4tc
```

**Stop the service**:
```cmd
net stop log4tc
```

**Restart the service**:
```cmd
net stop log4tc && net start log4tc
```

**Alternative using sc.exe**:
```cmd
sc start log4tc
sc stop log4tc
sc query log4tc
```

### Service Configuration

**Set automatic startup**:
```cmd
sc config log4tc start= auto
```

**Set manual startup** (start only when needed):
```cmd
sc config log4tc start= demand
```

**Disable the service** (will not start even at boot):
```cmd
sc config log4tc start= disabled
```

**Set service dependencies** (if needed):
```cmd
sc config log4tc depend= Tcpip/DNS
```

### Running as Specific User

By default, the service runs as `SYSTEM` (highest privileges). To run as a specific user:

**Create a service account** (if not existing):
```powershell
# PowerShell (as Administrator)
$password = ConvertTo-SecureString "SecurePassword123!" -AsPlainText -Force
New-LocalUser -Name "log4tc_svc" -Password $password -FullName "Log4TC Service Account" `
  -Description "Service account for Log4TC" -PasswordNeverExpires
```

**Assign necessary permissions**:
```powershell
# Add to local administrators (if needed for ADS protocol)
Add-LocalGroupMember -Group "Administrators" -Member "log4tc_svc"

# Or add specific permissions to log directory
icacls "C:\ProgramData\log4tc" /grant "log4tc_svc:(OI)(CI)F"
```

**Configure service to run as this user**:
```cmd
sc config log4tc obj= ".\log4tc_svc" password= "SecurePassword123!"
```

### Recovery Options

Configure automatic recovery if the service crashes:

**Set recovery actions**:
```cmd
sc failure log4tc reset= 86400 actions= restart/5000/restart/5000/restart/60000
```

This means:
- First failure: Restart after 5 seconds
- Second failure: Restart after 5 seconds
- Third failure: Restart after 60 seconds
- Reset failure count every 24 hours (86400 seconds)

**Alternative using sc.exe GUI**:
1. Open Services (services.msc)
2. Right-click on "log4tc"
3. Properties → Recovery tab
4. Set actions for "First failure", "Second failure", "Subsequent failures"

### Event Log Integration

Log4TC automatically logs to Windows Event Log under:
- **Event Viewer** → Windows Logs → Application
- **Source**: `log4tc`

**View service events**:
```powershell
Get-EventLog -LogName Application -Source log4tc -Newest 50
```

**Monitor in real-time**:
```powershell
Get-EventLog -LogName Application -Source log4tc -Newest 1 -AsBaseObject | Format-List
```

### Service Status Check

**Detailed status**:
```powershell
Get-Service -Name log4tc | Select-Object Status, StartType
```

**Continuous monitoring**:
```powershell
while ($true) {
    $status = (Get-Service log4tc).Status
    Write-Host "$(Get-Date): Service is $status"
    Start-Sleep -Seconds 5
}
```

---

## Docker Deployment

While the log4tc service itself is a native Windows service, you often need to deploy an OpenTelemetry Collector and backend services. Docker Compose can orchestrate these.

### Architecture

```
Windows Host
├── log4tc service (native .exe)
│   └─ Listens on :16150 (ADS), :4317 (OTLP)
│
└── Docker (on Windows)
    ├── OTEL Collector
    │   └─ Receives from log4tc
    ├── Grafana Loki (log storage)
    ├── Grafana (visualization)
    └── Prometheus (metrics)
```

### Docker Compose File

**File**: `docker-compose.yml`

```yaml
version: '3.8'

services:
  # OpenTelemetry Collector
  otel-collector:
    image: otel/opentelemetry-collector:latest
    container_name: otel-collector
    ports:
      - "4317:4317"    # OTLP gRPC
      - "4318:4318"    # OTLP HTTP
      - "55679:55679"  # Collector metrics
    volumes:
      - ./otel-collector-config.yaml:/etc/otel-collector-config.yaml
    command: ["--config=/etc/otel-collector-config.yaml"]
    environment:
      - GOGC=80
    networks:
      - log4tc-network

  # Grafana Loki - Log aggregation
  loki:
    image: grafana/loki:latest
    container_name: loki
    ports:
      - "3100:3100"
    volumes:
      - loki-storage:/loki
      - ./loki-config.yaml:/etc/loki/local-config.yaml
    command: -config.file=/etc/loki/local-config.yaml
    networks:
      - log4tc-network

  # Grafana - Visualization
  grafana:
    image: grafana/grafana:latest
    container_name: grafana
    ports:
      - "3000:3000"
    environment:
      - GF_SECURITY_ADMIN_PASSWORD=admin
      - GF_USERS_ALLOW_SIGN_UP=false
    volumes:
      - grafana-storage:/var/lib/grafana
      - ./grafana-datasources.yaml:/etc/grafana/provisioning/datasources/datasources.yaml
    depends_on:
      - loki
    networks:
      - log4tc-network

  # Prometheus - Metrics storage
  prometheus:
    image: prom/prometheus:latest
    container_name: prometheus
    ports:
      - "9090:9090"
    volumes:
      - ./prometheus.yaml:/etc/prometheus/prometheus.yml
      - prometheus-storage:/prometheus
    command:
      - '--config.file=/etc/prometheus/prometheus.yml'
      - '--storage.tsdb.path=/prometheus'
    networks:
      - log4tc-network

volumes:
  loki-storage:
  grafana-storage:
  prometheus-storage:

networks:
  log4tc-network:
    driver: bridge
```

### OTEL Collector Config

**File**: `otel-collector-config.yaml`

```yaml
receivers:
  otlp:
    protocols:
      grpc:
        endpoint: 0.0.0.0:4317
      http:
        endpoint: 0.0.0.0:4318

  prometheus:
    config:
      scrape_configs:
        - job_name: 'log4tc'
          static_configs:
            - targets: ['host.docker.internal:8888']  # log4tc metrics

processors:
  batch:
    send_batch_size: 512
    timeout: 5s

  memory_limiter:
    check_interval: 1s
    limit_mib: 512
    spike_limit_mib: 128

exporters:
  logging:
    loglevel: info

  loki:
    endpoint: http://loki:3100/loki/api/v1/push
    labels:
      job: log4tc

  prometheus:
    endpoint: 0.0.0.0:8888

service:
  pipelines:
    logs:
      receivers: [otlp]
      processors: [batch, memory_limiter]
      exporters: [loki, logging]
    
    metrics:
      receivers: [otlp, prometheus]
      processors: [batch, memory_limiter]
      exporters: [prometheus, logging]
```

### Loki Config

**File**: `loki-config.yaml`

```yaml
auth_enabled: false

ingester:
  chunk_idle_period: 3m
  max_chunk_age: 1h
  max_streams_per_user: 0
  max_global_streams_per_user: 0

limits_config:
  enforce_metric_name: false
  reject_old_samples: true
  reject_old_samples_max_age: 168h

schema_config:
  configs:
    - from: 2020-10-24
      store: boltdb-shipper
      object_store: filesystem
      schema: v11
      index:
        prefix: index_
        period: 24h

server:
  http_listen_port: 3100
  log_level: info

storage_config:
  boltdb_shipper:
    active_index_directory: /loki/boltdb-shipper-active
    cache_location: /loki/boltdb-shipper-cache
  filesystem:
    directory: /loki/chunks

chunk_store_config:
  max_look_back_period: 0s

table_manager:
  retention_deletes_enabled: false
  retention_period: 0s
```

### Grafana Datasources

**File**: `grafana-datasources.yaml`

```yaml
apiVersion: 1

datasources:
  - name: Loki
    type: loki
    access: proxy
    url: http://loki:3100
    isDefault: true

  - name: Prometheus
    type: prometheus
    access: proxy
    url: http://prometheus:9090
```

### Prometheus Config

**File**: `prometheus.yaml`

```yaml
global:
  scrape_interval: 15s
  evaluation_interval: 15s

scrape_configs:
  - job_name: 'otel-collector'
    static_configs:
      - targets: ['otel-collector:55679']

  - job_name: 'log4tc'
    static_configs:
      - targets: ['host.docker.internal:8888']
```

### Starting Docker Stack

```bash
# Start all services
docker-compose up -d

# View logs
docker-compose logs -f

# Stop services
docker-compose down

# Clean up volumes
docker-compose down -v
```

### Accessing Services

- **Grafana**: http://localhost:3000 (admin/admin)
- **Loki**: http://localhost:3100
- **Prometheus**: http://localhost:9090
- **OTEL Collector**: localhost:4317 (gRPC), localhost:4318 (HTTP)

---

## Monitoring & Health Checks

### Health Check Endpoint

The service exposes a health check endpoint for monitoring.

**Endpoint**: `http://localhost:8888/health`

**Response (healthy)**:
```json
{
  "status": "ok",
  "version": "1.0.0",
  "uptime_seconds": 3600,
  "timestamp": "2024-03-31T10:30:45Z"
}
```

**Response (unhealthy)**:
```json
{
  "status": "degraded",
  "errors": [
    "OTEL export queue at 95% capacity",
    "ADS connection count: 50/100"
  ],
  "timestamp": "2024-03-31T10:30:45Z"
}
```

### Metrics Endpoint

Prometheus-compatible metrics at: `http://localhost:8888/metrics`

**Key Metrics**:

| Metric | Type | Description |
|--------|------|-------------|
| `log4tc_messages_received_total` | Counter | Total messages received from all protocols |
| `log4tc_ads_messages_total` | Counter | Messages via ADS protocol |
| `log4tc_otel_messages_total` | Counter | Messages via OTEL protocol |
| `log4tc_export_success_total` | Counter | Successful exports |
| `log4tc_export_errors_total` | Counter | Failed exports |
| `log4tc_export_latency_ms` | Histogram | Export latency in milliseconds |
| `log4tc_queue_depth` | Gauge | Current queue depth |
| `log4tc_connections_active` | Gauge | Active connections |
| `log4tc_batch_size` | Histogram | Batch sizes processed |

**Example scrape config for Prometheus**:
```yaml
scrape_configs:
  - job_name: 'log4tc'
    static_configs:
      - targets: ['localhost:8888']
    scrape_interval: 15s
```

### Querying Metrics

**In Prometheus UI** (http://localhost:9090):
```promql
# Messages per second
rate(log4tc_messages_received_total[1m])

# Export error rate
rate(log4tc_export_errors_total[5m])

# Current queue depth
log4tc_queue_depth

# Export latency (p95)
histogram_quantile(0.95, rate(log4tc_export_latency_ms_bucket[5m]))
```

### Windows Performance Counters

Log4TC also registers Windows Performance Monitor counters.

**View in Performance Monitor** (perfmon.msc):
1. Performance Monitor
2. Add Counters
3. Object: `log4tc`
4. Available counters:
   - Messages Received/sec
   - Messages Exported/sec
   - Export Errors/sec
   - Queue Depth
   - Active Connections

### Dashboard Setup (Grafana)

Example Grafana dashboard JSON (create in Grafana UI):

1. **Panel 1**: Messages Received (rate)
   ```promql
   rate(log4tc_messages_received_total[1m])
   ```

2. **Panel 2**: Export Errors
   ```promql
   rate(log4tc_export_errors_total[5m])
   ```

3. **Panel 3**: Export Latency
   ```promql
   histogram_quantile(0.95, rate(log4tc_export_latency_ms_bucket[5m]))
   ```

4. **Panel 4**: Recent Logs (from Loki)
   ```logql
   {job="log4tc"} | json | level="error"
   ```

---

## Troubleshooting

### Service Won't Start

**Check logs**:
```powershell
Get-EventLog -LogName Application -Source log4tc -Newest 10 | Format-List
```

**Common issues**:

1. **Port already in use**:
   ```cmd
   netstat -ano | findstr :16150
   netstat -ano | findstr :4317
   ```
   Solution: Change port in config.toml or stop conflicting process.

2. **Config file not found**:
   ```
   ERROR: Configuration file not found at C:\ProgramData\log4tc\config.toml
   ```
   Solution: Ensure config.toml exists in the right location.

3. **Permission denied**:
   ```
   ERROR: Cannot create log directory: Access Denied
   ```
   Solution: Check file permissions on `C:\ProgramData\log4tc`
   ```cmd
   icacls C:\ProgramData\log4tc /grant "SYSTEM:(OI)(CI)F"
   ```

### No Messages Being Received

1. **Check ADS listener**:
   ```powershell
   Get-NetTCPConnection -LocalPort 16150 -State Listening
   ```
   
2. **Enable debug logging**:
   ```toml
   [service]
   log_level = "debug"
   
   [logging]
   level = "debug"
   ```

3. **Check PLC connectivity**:
   - Verify TwinCAT runtime is running on PLC
   - Test ADS route: TwinCAT System Settings → ADS Routes
   - Check network connectivity: `ping <plc-ip>`

### Messages Being Lost

**Symptoms**: Not all messages appear in backend

**Solutions**:

1. **Increase queue size**:
   ```toml
   [otel.batch]
   max_queue_size = 5000  # increase from 2048
   ```

2. **Check export latency**:
   ```promql
   histogram_quantile(0.95, rate(log4tc_export_latency_ms_bucket[5m]))
   ```
   
   If > 10 seconds, export is too slow. Increase batch size or optimize backend.

3. **Monitor memory**:
   ```cmd
   tasklist /fi "IMAGENAME eq log4tc-service.exe" /v
   ```
   
   If using 500+ MB, increase memory on system or reduce batch_size.

### Export Errors

**Check error rate**:
```promql
rate(log4tc_export_errors_total[5m])
```

**Common causes**:

1. **Backend unreachable**:
   ```cmd
   Test-NetConnection -ComputerName localhost -Port 4317
   ```

2. **Authentication failed**:
   Add debug logs and check credentials:
   ```toml
   [logging]
   level = "debug"
   ```

3. **Schema mismatch**:
   Ensure backend expects OTLP format and correct version.

### High Latency

**Symptoms**: Delay between log generation and appearing in backend

**Steps**:

1. **Check batch processing**:
   ```promql
   histogram_quantile(0.95, rate(log4tc_export_latency_ms_bucket[5m]))
   ```

2. **Profile CPU**:
   ```powershell
   Get-Counter -Counter "\Process(log4tc-service)\% Processor Time" -Continuous
   ```

3. **Reduce batch delay**:
   ```toml
   [otel.batch]
   scheduled_delay_ms = 1000  # reduce from 5000
   ```

### Out of Memory

**Check memory usage**:
```powershell
Get-Process -Name "log4tc-service" | Select-Object WorkingSet
```

**Solutions**:

1. **Reduce queue size**:
   ```toml
   [otel.batch]
   max_queue_size = 1024  # reduce
   ```

2. **Reduce batch size**:
   ```toml
   [otel.batch]
   max_batch_size = 256  # reduce from 512
   ```

3. **Check for memory leak**:
   Monitor memory over time. If always increasing, file a bug report.

---

## Example Configurations

### Example 1: Minimal Configuration

**Use case**: Single TwinCAT PLC, basic logging

```toml
# config.toml - Minimal Setup

[service]
name = "log4tc"
log_level = "info"
worker_threads = 2

[ads]
enabled = true
port = 16150
bind_address = "0.0.0.0"

[otel]
enabled = false

[logging]
level = "info"
output = "file"
directory = "%ProgramData%/log4tc/logs"
```

### Example 2: Production Configuration

**Use case**: High-throughput production environment, multiple exporters

```toml
# config.toml - Production Setup

[service]
name = "log4tc-production"
log_level = "warn"
worker_threads = 8
graceful_shutdown_timeout = 60

[ads]
enabled = true
port = 16150
bind_address = "0.0.0.0"
max_connections = 200
buffer_size = 131072
max_message_size = 2097152

[otel]
enabled = false  # Not using OTEL yet
endpoint = "0.0.0.0:4317"
protocol = "grpc"
compression = "gzip"

[otel.batch]
enabled = true
max_batch_size = 1024
scheduled_delay_ms = 2000
max_queue_size = 5000
max_export_batch_size = 1024

[otel.retry]
enabled = true
initial_interval_ms = 1000
max_interval_ms = 60000
max_elapsed_time_ms = 600000
multiplier = 2.0

[otel.resource]
"service.name" = "log4tc"
"service.version" = "1.0.0"
"deployment.environment" = "production"
"service.instance.id" = "log4tc-prod-01"
"host.name" = "PROD-SERVER-01"

[logging]
level = "warn"
format = "json"
output = "both"
directory = "%ProgramData%/log4tc/logs"
max_file_size = 209715200  # 200 MB
max_backup_files = 20

[[exporters]]
type = "otlp"
endpoint = "http://otel-collector:4317"
protocol = "grpc"
timeout_seconds = 30

[[exporters]]
type = "loki"
endpoint = "http://loki:3100"
tenant_id = "production"
batch_size = 2000

[[exporters]]
type = "influxdb"
url = "http://influx.prod:8086"
database = "log4tc-prod"
retention_policy = "7d"
```

### Example 3: High-Throughput Configuration

**Use case**: Very high message volume (>10k logs/sec)

```toml
# config.toml - High Throughput

[service]
name = "log4tc-highperf"
log_level = "error"  # Only errors
worker_threads = 16
graceful_shutdown_timeout = 120

[ads]
enabled = true
port = 16150
bind_address = "0.0.0.0"
max_connections = 500
buffer_size = 262144  # 256 KB
max_message_size = 4194304  # 4 MB

[otel]
enabled = false

[otel.batch]
enabled = true
max_batch_size = 2048  # Large batches
scheduled_delay_ms = 1000  # Quick export
max_queue_size = 10000
max_export_batch_size = 2048

[otel.retry]
enabled = true
initial_interval_ms = 500
max_interval_ms = 10000
max_elapsed_time_ms = 120000

[logging]
level = "error"
format = "text"  # Faster than JSON
output = "file"
directory = "%ProgramData%/log4tc/logs"
max_file_size = 524288000  # 500 MB
max_backup_files = 30

[[exporters]]
type = "otlp"
endpoint = "http://otel-collector:4317"
protocol = "grpc"
timeout_seconds = 60

[[exporters]]
type = "loki"
endpoint = "http://loki:3100"
tenant_id = "default"
batch_size = 5000
```

### Example 4: Development/Debug Configuration

**Use case**: Local development with detailed logging

```toml
# config.toml - Development

[service]
name = "log4tc-dev"
log_level = "debug"
worker_threads = 2

[ads]
enabled = true
port = 16150
bind_address = "127.0.0.1"  # Local only
max_connections = 10

[otel]
enabled = true
endpoint = "127.0.0.1:4317"
protocol = "grpc"
compression = "none"

[otel.batch]
enabled = true
max_batch_size = 10  # Small for testing
scheduled_delay_ms = 500
max_queue_size = 100

[logging]
level = "trace"
format = "text"
output = "both"  # Console + file
directory = "C:/Temp/log4tc/logs"

[[exporters]]
type = "otlp"
endpoint = "http://127.0.0.1:4317"
protocol = "grpc"

# Optional: Echo to console for debugging
[[exporters]]
type = "logging"
output = "stdout"
```

### Example 5: With TLS Configuration

**Use case**: Secure OTLP receiver for remote clients

```toml
# config.toml - TLS Enabled

[service]
name = "log4tc-secure"
log_level = "info"
worker_threads = 4

[ads]
enabled = false  # Not using legacy protocol

[otel]
enabled = true
endpoint = "0.0.0.0:4317"
protocol = "grpc"
compression = "gzip"
timeout_seconds = 30

[otel.tls]
enabled = true
cert_path = "C:\\certs\\server.crt"
key_path = "C:\\certs\\server.key"
client_auth_required = true
ca_path = "C:\\certs\\ca.crt"

[otel.headers]
"Authorization" = "Bearer token_abc123"

[logging]
level = "info"
output = "file"
directory = "%ProgramData%/log4tc/logs"

[[exporters]]
type = "otlp"
endpoint = "https://otel-collector:4317"
protocol = "grpc"
timeout_seconds = 30
```

---

## Additional Resources

### Documentation
- [OpenTelemetry Specification](https://opentelemetry.io/docs/specs/)
- [OTLP Protocol](https://github.com/open-telemetry/opentelemetry-specification/blob/main/specification/protocol/otlp.md)
- [Message Templates](https://messagetemplates.org/)

### Tools
- [OTEL CLI](https://github.com/open-telemetry/opentelemetry-cli) - Debug OTLP traffic
- [Grafana Loki](https://grafana.com/oss/loki/) - Log aggregation
- [Prometheus](https://prometheus.io/) - Metrics collection
- [Grafana](https://grafana.com/) - Visualization

### Support
- GitHub Issues: [log4TC Repository](https://github.com)
- Documentation: [Log4TC Wiki]
- Community: [TwinCAT Forum]


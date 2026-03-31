# Log4TC Container Quick Start Guide

## Overview

This guide explains how to run Log4TC in a container (Docker or Podman) when you **do not have an ADS router** on the host machine. The service accepts raw TCP connections on port 16150 directly from your TwinCAT PLC.

## Architecture

```
TwinCAT PLC (VM)
    ↓
    └─→ TCP port 16150 (HOST_IP)
            ↓
            Container Host (Docker or Podman)
            ↓
    log4tc-service container (0.0.0.0:16150)
            ↓
    OpenTelemetry Collector (logs exporter)
            ↓
            stdout/logging output
```

## Key Points

- **No ADS Router Required**: The Rust service implements a lightweight ADS TCP server
- **Direct TCP Connection**: The PLC sends raw binary ADS frames to the container host IP on port 16150
- **0.0.0.0 Binding**: The service binds to all interfaces (0.0.0.0) inside the container, allowing external connections
- **Container Port Mapping**: Port 16150 is mapped from the container to the host (works with both Docker and Podman)
- **Rootless by Default**: Podman runs in rootless mode for better security

## Quick Start

> **Note**: Choose either **Docker** or **Podman** below. Both are API-compatible and work identically.

### 1. Build the Image

**Docker:**
```bash
docker build -t log4tc .
```

**Podman:**
```bash
podman build -t log4tc .
```

### 2. Start the Services

**Docker:**
```bash
docker-compose up
```

**Podman (with podman-compose):**
```bash
podman-compose up
```

**Podman (native):**
```bash
podman compose up
```

This starts:
- **log4tc-service**: Listens on port 16150 for ADS messages
- **otel-collector**: Receives OpenTelemetry logs on port 4318, outputs to stdout

### 3. Configure Your TwinCAT PLC

In your PLC code or TwinCAT IDE:

1. Set the target AMS Net ID to your container host IP
   - Example: `192.168.1.100` (the machine running Docker or Podman)

2. Connect to port 16150

3. Send ADS log messages using the standard TwinCAT logging API

### 4. Verify It Works

**Docker:**
```bash
docker-compose logs -f log4tc-service
```

**Podman:**
```bash
podman-compose logs -f log4tc-service
# or
podman compose logs -f log4tc-service
```

You should see:
- Service startup message: "ADS listener started on 0.0.0.0:16150"
- New connections from your PLC IP
- Log entries being received and processed

## Configuration Files

### config.docker.json
- **Receiver Host**: 0.0.0.0 (accepts external connections)
- **ADS Port**: 16150 (can be changed in docker-compose.yml)
- **Logging Output**: stdout (visible in docker-compose logs)
- **OTLP HTTP Port**: 4318

### docker-compose.yml
- Maps port 16150 from container to host for ADS
- Maps port 4318 for OTLP HTTP receiver (optional external access)
- Includes health checks for both services
- Volume mounts for config and collector configuration

### otel-collector-config.yml
- Receives OTLP logs on port 4318 (HTTP and gRPC)
- Exports logs to stdout for visibility
- Applies service.name attribute "log4tc"

## Network Modes

### Default: Bridge Network
- Containers can communicate with each other
- Ports are mapped to the host
- **Recommended for most setups**

### Alternative: Host Network (Linux only)
For maximum simplicity, you can use host network mode in docker-compose.yml:

```yaml
log4tc-service:
  network_mode: "host"
  ports: []  # Remove port mapping
```

With host network, the container shares the host's network stack, but this only works on Linux.

## Troubleshooting

### "Connection refused" from PLC

1. Verify the container host IP: `ipconfig` (Windows) or `ifconfig` (Linux)
2. Ensure no firewall blocks port 16150
3. Check service is running:
   - Docker: `docker-compose ps`
   - Podman: `podman-compose ps` or `podman compose ps`
4. View logs:
   - Docker: `docker-compose logs log4tc-service`
   - Podman: `podman-compose logs log4tc-service` or `podman compose logs log4tc-service`

### "Address already in use"

Port 16150 is already in use on your host:

```bash
# Find what's using port 16150
netstat -ano | findstr :16150  # Windows
lsof -i :16150                 # Linux/Mac

# Change port in docker-compose.yml and redeploy
```

**For Podman rootless mode:** Port numbers below 1024 require special configuration. If using ports < 1024, check Podman documentation for `net.ipv4.ip_unprivileged_port_start`.

### No logs appearing

1. Check PLC is sending to the correct IP and port 16150
2. Verify otel-collector is healthy:
   - Docker: `docker-compose logs otel-collector`
   - Podman: `podman-compose logs otel-collector` or `podman compose logs otel-collector`
3. Check OTLP endpoint in your PLC logging config is 127.0.0.1:4318 (optional)

## Stopping and Cleanup

**Docker:**
```bash
# Stop containers
docker-compose down

# Remove images (optional)
docker rmi log4tc otel/opentelemetry-collector:latest
```

**Podman:**
```bash
# Stop containers
podman-compose down
# or
podman compose down

# Remove images (optional)
podman rmi log4tc otel/opentelemetry-collector:latest
```

## Common Issues and Solutions

### Issue: Logs not appearing in docker-compose output

**Solution**: Ensure logging output is set to "stdout" in config.docker.json:

```json
"logging": {
  "outputPath": "stdout"
}
```

### Issue: Service crashes on startup

1. Check the config file is valid JSON:
   - Docker: `docker-compose logs log4tc-service`
   - Podman: `podman-compose logs log4tc-service`
2. Ensure config.docker.json exists and is mounted correctly
3. Verify the service binary has execute permissions

### Podman Rootless Specific Issues

**Issue: Port binding fails**

Podman rootless mode restricts port binding. For ports above 1024, this usually works automatically. For ports below 1024 or if you hit limits:

```bash
# Check current unprivileged port range
cat /proc/sys/net/ipv4/ip_unprivileged_port_start

# Modify if needed (as root):
sudo sysctl -w net.ipv4.ip_unprivileged_port_start=0
```

**Issue: Permission denied on volume mounts**

Ensure Podman has read access to mounted files:

```bash
# Make config world-readable
chmod 644 config.docker.json otel-collector-config.yml
```

### Issue: Multiple PLC connections cause slowdown

Adjust in config.docker.json:

```json
"service": {
  "channelCapacity": 50000  // Increase buffer
}
```

Also check `crates/log4tc-ads/src/listener.rs` for `DEFAULT_MAX_CONNECTIONS` constant.

## Next Steps

- Configure your log outputs in config.docker.json (nlog, graylog, etc.)
- Set up persistent storage for logs using Docker volumes
- Monitor service health with the built-in health checks
- Integrate with your log aggregation platform via the OTLP receiver

# Log4TC Container Quick Start Guide

## Overview

This guide explains how to run Log4TC in a container (Docker or Podman) when you **do not have an ADS router** on the host machine. The service accepts log entries via two protocols:

1. **AMS/TCP Server (Recommended)**: Port 48898 - ADS Write commands routed through AMS/TCP from your PLC's ADS Router
2. **Legacy ADS TCP**: Port 16150 - Raw TCP connections for backward compatibility

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
- **log4tc-service**: Listens on port 48898 (AMS/TCP) and 16150 (legacy ADS) for log messages
- **otel-collector**: Receives OpenTelemetry logs on port 4318, outputs to stdout

### 3. Configure Your TwinCAT PLC

#### Option A: AMS/TCP Route (Recommended)

This method uses the PLC's built-in ADS Router to connect via AMS/TCP:

1. In the TwinCAT IDE, create a new **AMS Route** in your project:
   - **Route Name**: `log4tc-docker` (or any name)
   - **AMS Net ID**: Set to the container host's Docker internal IP + `.1.1`
     - Example: `172.17.0.2.1.1` (depends on your Docker network)
     - For Podman: Check the network using `podman network inspect podman`
   - **Transport Type**: TCP/IP
   - **Address**: Container host IP (e.g., `192.168.1.100` or the Docker/Podman gateway IP)
   - **Port**: 48898 (default AMS/TCP port)

2. In your PLC code, use the standard TwinCAT logging API - it will automatically route through the configured AMS route

3. Logs will be transmitted via AMS/TCP port 48898

#### Option B: Legacy Direct ADS (Port 16150)

For backward compatibility, the service still listens on port 16150 for direct ADS connections:

1. Configure your PLC logging to send directly to port 16150
2. This method bypasses the AMS Router

#### Get Your Docker/Podman IP

**Docker:**
```bash
docker inspect -f '{{range .NetworkSettings.Networks}}{{.Gateway}}{{end}}' log4tc-service
```

**Podman:**
```bash
podman inspect log4tc-service -f '{{.NetworkSettings.Gateway}}'
# or check the network
podman network inspect podman | grep -A 20 gateway_ipv4
```

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
- Service startup messages for both AMS/TCP server and ADS listener
- Specifically: "AMS/TCP server listening on 0.0.0.0:48898"
- New connections from your PLC IP
- Log entries being received and processed

## Configuration Files

### config.docker.json
- **Receiver Host**: 0.0.0.0 (accepts external connections)
- **AMS Net ID**: 172.17.0.2.1.1 (Docker internal IP - adjust for your setup)
- **AMS/TCP Port**: 48898 (for AMS routed ADS commands)
- **ADS Port**: 16150 (for legacy direct ADS connections)
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

1. Verify the container host IP (for AMS Net ID):
   - `ipconfig` (Windows) or `ifconfig` (Linux)
   - Get the Docker/Podman gateway IP (see **Get Your Docker/Podman IP** above)
2. Ensure no firewall blocks ports 48898 (AMS/TCP) and/or 16150 (legacy ADS)
3. Check AMS Net ID configuration matches your container setup
4. Check service is running:
   - Docker: `docker-compose ps`
   - Podman: `podman-compose ps` or `podman compose ps`
5. View logs:
   - Docker: `docker-compose logs -f log4tc-service`
   - Podman: `podman-compose logs -f log4tc-service` or `podman compose logs -f log4tc-service`

### "Address already in use"

Port 48898 or 16150 is already in use on your host:

```bash
# Find what's using the port
netstat -ano | findstr :48898  # Windows (AMS/TCP)
netstat -ano | findstr :16150  # Windows (legacy ADS)
lsof -i :48898                 # Linux/Mac
lsof -i :16150

# Change port in docker-compose.yml and redeploy
```

**For Podman rootless mode:** Port numbers below 1024 require special configuration. If using ports < 1024, check Podman documentation for `net.ipv4.ip_unprivileged_port_start`.

### AMS Net ID Mismatch

The AMS Net ID in config.docker.json must match the Docker/Podman gateway IP:

```bash
# Get current gateway IP
docker inspect -f '{{range .NetworkSettings.Networks}}{{.Gateway}}{{end}}' log4tc-service
# or
podman inspect log4tc-service -f '{{.NetworkSettings.Gateway}}'

# Update config.docker.json if needed (format: X.X.X.X.1.1)
```

If the AMS Net ID is wrong, the PLC's ADS Router won't find the route, causing connection failures.

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

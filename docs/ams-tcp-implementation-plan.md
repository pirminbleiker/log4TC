# AMS/TCP Server Implementation Plan

## Ziel

Implementierung eines AMS/TCP Servers in Rust, der auf Port 48898 lauscht und ADS Write-Befehle von TwinCAT PLCs empfängt. Ersetzt den bisherigen Raw-TCP Listener auf Port 16150. Kein ADS-Router auf dem Host nötig.

## Architektur

```
TwinCAT PLC (VM)                        Host (Podman Container)
┌─────────────────┐                     ┌──────────────────────────────┐
│ FB_Log4TcTask    │                     │ log4tc-service               │
│                  │  AMS/TCP            │                              │
│ ADS Router ──────┼──Port 48898───────► │ AmsTcpServer (:48898)        │
│ (in VM)          │                     │   ├── parse AMS/TCP frame    │
│                  │                     │   ├── extract ADS Write data │
│ AMS Net ID:      │  ◄── Response ──── │   ├── route to AdsParser     │
│ 5.x.x.x.1.1     │                     │   └── send ADS response     │
└─────────────────┘                     │         │                    │
                                        │         ▼                    │
                                        │   LogEntry → OTEL → Collector│
                                        └──────────────────────────────┘
```

## AMS/TCP Frame Format

### Request (PLC → Server)

```
Offset  Size  Field                  Value
──────  ────  ─────                  ─────
 0       2    TCP Reserved           0x0000
 2       4    TCP Data Length        (u32 LE, = AMS Header + ADS Data)
 6       6    Target AMS Net ID     Server Net ID (z.B. 192.168.1.100.1.1)
12       2    Target AMS Port       16150 (u16 LE)
14       6    Source AMS Net ID     PLC Net ID
20       2    Source AMS Port       PLC Port
22       2    Command ID            0x0003 (Write)
24       2    State Flags           0x0004 (Request + ADS Command)
26       4    Data Length           (u32 LE, ADS payload size)
30       4    Error Code            0x00000000
34       4    Invoke ID             (u32 LE, correlation token)
── ADS Write Payload ──
38       4    Index Group           1 (u32 LE)
42       4    Index Offset          1 (u32 LE)
46       4    Write Data Length     (u32 LE)
50       N    Write Data            AdsLogEntry binary (Version + Message + ...)
```

### Response (Server → PLC)

```
Offset  Size  Field                  Value
──────  ────  ─────                  ─────
 0       2    TCP Reserved           0x0000
 2       4    TCP Data Length        36 (32 AMS header + 4 result)
 6       6    Target AMS Net ID     PLC Net ID (swap source/target)
12       2    Target AMS Port       PLC Port
14       6    Source AMS Net ID     Server Net ID
20       2    Source AMS Port       16150
22       2    Command ID            0x0003 (Write)
24       2    State Flags           0x0005 (Response + ADS Command)
26       4    Data Length           4
30       4    Error Code            0x00000000
34       4    Invoke ID             (echo from request)
── ADS Write Response ──
38       4    Result                0x00000000 (success)
```

## Implementation Tasks

### Task 1: AMS Protocol Types (`crates/log4tc-ads/src/ams.rs`) - NEW FILE

Structs und Parsing für AMS/TCP:

```rust
pub const AMS_TCP_PORT: u16 = 48898;
pub const ADS_CMD_WRITE: u16 = 3;
pub const ADS_STATE_REQUEST: u16 = 0x0004;
pub const ADS_STATE_RESPONSE: u16 = 0x0005;

#[derive(Debug, Clone)]
pub struct AmsNetId([u8; 6]);

#[derive(Debug)]
pub struct AmsHeader {
    pub target_net_id: AmsNetId,
    pub target_port: u16,
    pub source_net_id: AmsNetId,
    pub source_port: u16,
    pub command_id: u16,
    pub state_flags: u16,
    pub data_length: u32,
    pub error_code: u32,
    pub invoke_id: u32,
}

#[derive(Debug)]
pub struct AdsWriteRequest {
    pub index_group: u32,
    pub index_offset: u32,
    pub data: Vec<u8>,
}

impl AmsHeader {
    pub fn parse(data: &[u8]) -> Result<Self>;
    pub fn serialize(&self) -> Vec<u8>;
    pub fn make_response(&self, error_code: u32) -> Self;
}

impl AmsNetId {
    pub fn from_str(s: &str) -> Result<Self>;  // "192.168.1.100.1.1"
    pub fn to_string(&self) -> String;
}
```

### Task 2: AMS/TCP Server (`crates/log4tc-ads/src/ams_server.rs`) - NEW FILE

Async TCP Server auf Port 48898:

```rust
pub struct AmsTcpServer {
    net_id: AmsNetId,       // Server's AMS Net ID
    port: u16,              // 48898
    ads_port: u16,          // 16150 (ADS port to accept)
    log_tx: mpsc::Sender<LogEntry>,
}

impl AmsTcpServer {
    pub fn new(net_id: AmsNetId, log_tx: mpsc::Sender<LogEntry>) -> Self;
    pub async fn start(&self) -> Result<()>;
    async fn handle_connection(stream: TcpStream, ...);
    async fn handle_frame(data: &[u8], ...) -> Result<Vec<u8>>;
}
```

Flow:
1. Accept TCP connection
2. Read 6-byte AMS/TCP header → get data length
3. Read AMS header (32 bytes) + payload
4. If Command ID == Write && Target Port == 16150:
   - Extract Write payload (skip 12 bytes IndexGroup/Offset/Length)
   - Pass to existing `AdsParser::parse()`
   - Convert to LogEntry, send via channel
5. Build response frame, send back
6. Keep connection open for more frames

### Task 3: Integration & Config Updates

- Update `crates/log4tc-ads/src/lib.rs` to export new modules
- Update `crates/log4tc-service/src/service.rs`:
  - Start AmsTcpServer on 48898 instead of/alongside AdsListener on 16150
  - Add AMS Net ID config
- Update `crates/log4tc-core/src/config.rs`:
  - Add `ams_net_id: String` to config
  - Add `ams_tcp_port: u16` (default 48898)
- Update `Dockerfile`: EXPOSE 48898
- Update `docker-compose.yml`: map port 48898
- Update `config.docker.json` with AMS settings

### Task 4: Tests

- Unit tests for AMS frame parsing (known byte sequences)
- Unit tests for response building
- Unit test for ADS Write extraction
- Integration test: send AMS/TCP Write frame → verify LogEntry arrives

## Config Beispiel

```json
{
  "receiver": {
    "host": "0.0.0.0",
    "amsNetId": "172.17.0.2.1.1",
    "amsTcpPort": 48898,
    "adsPort": 16150
  }
}
```

## PLC-Seite Konfiguration

In der TwinCAT VM eine Route anlegen:
- Name: `log4tc-docker`
- AMS Net Id: `<Host-IP>.1.1` (muss mit config.amsNetId übereinstimmen)
- Transport: TCP/IP
- Address: Host-IP (z.B. 192.168.1.100)

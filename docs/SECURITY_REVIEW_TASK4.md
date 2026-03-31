# Security Review: Task #4 - ADS TCP Listener

**Reviewer**: Security Expert  
**Date**: 2026-03-31  
**Status**: Implementation review completed  
**Severity Summary**: 2 HIGH, 3 MEDIUM findings

---

## Findings

### 1. 🔴 HIGH: No Max Message Size Enforcement

**Location**: `crates/log4tc-ads/src/parser.rs:134-143` (read_string)

**Issue**:
```rust
fn read_string(&mut self) -> Result<String> {
    let len_bytes = self.read_bytes(2)?;
    let len = u16::from_le_bytes([...]) as usize;
    let str_bytes = self.read_bytes(len)?;  // No limit check!
    String::from_utf8(str_bytes.to_vec()).map_err(...)
}
```

The parser reads string length as u16 without checking against configured limits. A malicious packet claiming 64KB strings for each field (message, logger, task_name, app_name, project_name, context keys...) can allocate several MB per packet.

**Impact**: Denial of Service via memory exhaustion.

**Fix Required**:
```rust
const MAX_STRING_LENGTH: usize = 65536; // 64KB per string

fn read_string(&mut self) -> Result<String> {
    let len_bytes = self.read_bytes(2)?;
    let len = u16::from_le_bytes([...]) as usize;
    
    if len > MAX_STRING_LENGTH {
        return Err(AdsError::StringTooLarge {
            requested: len,
            max: MAX_STRING_LENGTH,
        });
    }
    
    let str_bytes = self.read_bytes(len)?;
    String::from_utf8(str_bytes.to_vec())
        .map_err(|e| AdsError::InvalidStringEncoding(e.to_string()))
}
```

**Configuration**: Add to config to allow per-deployment limits:
```toml
[ads]
max_string_length = 65536      # bytes
max_arguments_per_entry = 32   # count
max_context_vars_per_entry = 64 # count
```

---

### 2. 🔴 HIGH: Unbounded HashMap Growth in Arguments/Context

**Location**: `crates/log4tc-ads/src/parser.rs:55-73`

**Issue**:
```rust
loop {
    let type_id = reader.read_u8()?;
    if type_id == 0 {
        break;
    }
    
    if type_id == 1 {
        let index = reader.read_u8()?;  // Can be 0-255
        arguments.insert(index as usize, value);
    } else if type_id == 2 {
        let scope = reader.read_u8()?;
        let name = reader.read_string()?;
        let value = reader.read_value()?;
        context.insert(format!("scope_{}_{}",scope, name), value);
    }
}
```

Parser allows up to 256 arguments (u8 index) and unlimited context variables. If each value is large (e.g., 1MB strings), total message could be > 256MB.

**Impact**: Denial of Service via memory exhaustion.

**Fix Required**:
```rust
const MAX_ARGUMENTS: usize = 32;
const MAX_CONTEXT_VARS: usize = 64;

// In parse loop:
if arguments.len() >= MAX_ARGUMENTS {
    return Err(AdsError::TooManyArguments {
        found: arguments.len() + 1,
        max: MAX_ARGUMENTS,
    });
}

if context.len() >= MAX_CONTEXT_VARS {
    return Err(AdsError::TooManyContextVars {
        found: context.len() + 1,
        max: MAX_CONTEXT_VARS,
    });
}
```

---

### 3. 🟠 MEDIUM: No Total Message Size Tracking

**Location**: `crates/log4tc-ads/src/parser.rs:15-91` (parse method)

**Issue**: Parser doesn't track total bytes consumed. A malicious client can send partial data repeatedly without triggering message size limits.

**Example Attack**:
```
1. Send 65KB message header (message field)
2. Send 65KB logger field
3. ... repeat 20 times ...
-> Total: 1.3MB despite per-field limits
```

**Fix Required**:
```rust
const MAX_MESSAGE_SIZE: usize = 1_048_576; // 1 MB per message

pub struct AdsParser {
    max_message_size: usize,
}

fn parse_from_reader(reader: &mut BytesReader) -> Result<AdsLogEntry> {
    let start_pos = reader.pos;
    
    // ... parsing ...
    
    let bytes_consumed = reader.pos - start_pos;
    if bytes_consumed > self.max_message_size {
        return Err(AdsError::MessageTooLarge {
            size: bytes_consumed,
            max: self.max_message_size,
        });
    }
    
    Ok(entry)
}
```

---

### 4. 🟠 MEDIUM: No Connection Limits in Listener

**Location**: `crates/log4tc-ads/src/listener.rs:26-47`

**Issue**:
```rust
pub async fn start(&self) -> Result<()> {
    let listener = TcpListener::bind(&addr).await?;
    
    loop {
        let (socket, peer_addr) = listener.accept().await?;
        let log_tx = self.log_tx.clone();
        
        tokio::spawn(async move {  // NO LIMIT - spawns unlimited tasks!
            if let Err(e) = Self::handle_connection(socket, peer_addr, log_tx).await {
                tracing::error!("Connection error from {}: {}", peer_addr, e);
            }
        });
    }
}
```

Listener accepts unlimited concurrent connections. An attacker can open 10,000 connections simultaneously, each with a 64KB buffer = 640MB memory, or send data slowly causing timeouts to pile up.

**Impact**: Denial of Service via resource exhaustion.

**Fix Required**:
```rust
pub struct AdsListener {
    host: String,
    port: u16,
    log_tx: mpsc::Sender<LogEntry>,
    max_connections: usize,
    connection_timeout_secs: u64,
}

// In start():
let mut active_connections = 0;
let max_connections = self.max_connections;

loop {
    let (socket, peer_addr) = listener.accept().await?;
    
    if active_connections >= max_connections {
        tracing::warn!("Max connections reached, rejecting {}", peer_addr);
        drop(socket); // Immediately close
        continue;
    }
    
    active_connections += 1;
    let log_tx = self.log_tx.clone();
    
    tokio::spawn(async move {
        // ... handle with timeout ...
        active_connections -= 1;
    });
}
```

**Configuration**:
```toml
[ads]
max_connections = 100          # Concurrent connections
connection_timeout_secs = 300  # 5 minutes
```

---

### 5. 🟠 MEDIUM: No Connection Timeout

**Location**: `crates/log4tc-ads/src/listener.rs:59-70`

**Issue**:
```rust
loop {
    let n = socket.read(&mut buffer).await?;  // No timeout!
    
    if n == 0 {
        break;
    }
    
    // ... parse ...
}
```

A client that connects but never sends data will cause the task to block indefinitely. Combined with unlimited connections (finding #4), this enables Slowloris-style DoS.

**Impact**: Resource exhaustion via slow/hung connections.

**Fix Required**:
```rust
use tokio::time::timeout;
use std::time::Duration;

const READ_TIMEOUT: Duration = Duration::from_secs(300);

loop {
    match timeout(READ_TIMEOUT, socket.read(&mut buffer)).await {
        Ok(Ok(n)) => {
            if n == 0 {
                tracing::debug!("Connection closed by {}", peer_addr);
                break;
            }
            // ... parse ...
        }
        Ok(Err(e)) => {
            tracing::warn!("Read error from {}: {}", peer_addr, e);
            break;
        }
        Err(_) => {
            tracing::warn!("Connection timeout from {}", peer_addr);
            break;
        }
    }
}
```

---

## Recommendations Summary

### MUST FIX Before Release (HIGH priority):
1. Add `MAX_STRING_LENGTH` constant (64KB) and enforce in parser
2. Add `MAX_ARGUMENTS` (32) and `MAX_CONTEXT_VARS` (64) limits with enforcement
3. Add total message size tracking with `MAX_MESSAGE_SIZE` (1MB) limit
4. Add `max_connections` limit and enforcement in listener
5. Add `connection_timeout_secs` (300s default) with tokio::time::timeout

### SHOULD FIX in v0.2 (MEDIUM priority):
6. Make max_* values configurable via AppSettings
7. Add Prometheus metrics: active_connections, total_messages, parse_errors
8. Add backpressure handling: drop connections if dispatcher queue full

### NICE TO HAVE (LOW priority):
9. Rate limiting per source IP
10. Connection-level integrity checks (CRC, length prefix)

---

## Testing Checklist

Before deploying:
- [ ] Unit test: parser rejects strings > 64KB
- [ ] Unit test: parser rejects messages > 1MB  
- [ ] Unit test: parser limits arguments to 32
- [ ] Integration test: 100 concurrent connections succeed
- [ ] Integration test: 101st connection rejected
- [ ] Fuzz test: parser doesn't panic on random bytes
- [ ] Stress test: send 10MB of garbage, verify memory bounded

---

## Security Approval

**Status**: ⚠️ CONDITIONAL - Awaiting fixes for HIGH findings

**Approval Path**:
1. Developer implements fixes for findings #1-5
2. Security expert reviews fixes and runs tests
3. Move to formal review (Task #11) only after approval

**Blockers**: Do NOT proceed to production without addressing findings #1, #2, #4, #5.

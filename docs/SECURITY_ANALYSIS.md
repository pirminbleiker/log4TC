# Security Analysis: Log4TC Rust Migration

**Document Version**: 1.0  
**Last Updated**: 2026-03-31  
**Audience**: Development team, Security review team

## Executive Summary

Log4TC is a logging bridge between Beckhoff TwinCAT PLCs and OpenTelemetry backends. The Rust rewrite introduces two network-exposed services:

1. **ADS Binary Protocol Listener** (Port 16150) - Legacy protocol from TwinCAT PLCs
2. **OTLP Receiver** (Port 4317/4318) - OpenTelemetry Protocol from external sources

Both services process untrusted network input, requiring careful implementation to avoid buffer overflows, DoS attacks, and credential exposure.

---

## Threat Model

### Attack Vectors

| Vector | Component | Risk | Mitigation |
|--------|-----------|------|-----------|
| **Malformed ADS Packets** | Parser | Buffer overflow, panic, DoS | Input validation, length checks, fuzzing |
| **Large Payloads** | Both listeners | Memory exhaustion, DoS | Max message size limits |
| **Slow/Incomplete Reads** | Both listeners | Resource exhaustion | Connection timeouts, backpressure |
| **Network Exposure** | ADS Listener | Unauthorized access | Firewall, bind address restrictions |
| **TLS Misconfiguration** | OTLP Exporter | MITM, credential leakage | Cert validation, TLS 1.2+, strong ciphers |
| **Log Injection** | Formatter | Injection attacks in log output | Output encoding, sanitization |
| **Container Escape** | Docker Container | Privilege escalation to host | Non-root user in container, seccomp profiles |

---

## Detailed Findings

### 1. ADS Listener (Task #4) - HIGH PRIORITY

#### 1.1 Bind Address Exposure

**Issue**: Configuration defaults allow binding to `0.0.0.0` (all interfaces):
```toml
[ads]
bind_address = "0.0.0.0"  # Accessible from any network
port = 16150
```

**Risk**: Exposes binary protocol parser to untrusted networks. In production, PLCs should communicate over controlled networks or localhost-only for single-machine setups.

**Recommendation**:
```rust
// Validation in listener creation
if bind_addr == "0.0.0.0" && !is_production_approved() {
    warn!("ADS listener bound to 0.0.0.0 - ensure firewall rules restrict access");
}

// Better default:
// bind_address = "127.0.0.1"  // Loopback only
// For distributed: use firewall + explicit IPs
```

#### 1.2 Buffer Overflow in String Parsing

**Location**: `crates/log4tc-ads/src/parser.rs:134-143`

**Issue**: String length prefix is read as u16, but no validation against configured limits:
```rust
fn read_string(&mut self) -> Result<String> {
    let len_bytes = self.read_bytes(2)?;
    let len = u16::from_le_bytes([len_bytes[0], len_bytes[1]]) as usize;
    let str_bytes = self.read_bytes(len)?;  // Can request up to 64KB without limit check
    String::from_utf8(str_bytes.to_vec()).map_err(|e| { ... })
}
```

**Attack**: Malicious packet claims a 1MB string length, parser allocates 1MB per field × 10 fields = 10MB per packet.

**Recommendation**: Enforce `max_message_size` configuration:
```rust
const MAX_STRING_LENGTH: usize = 65536; // 64KB reasonable limit

fn read_string(&mut self) -> Result<String> {
    let len_bytes = self.read_bytes(2)?;
    let len = u16::from_le_bytes([len_bytes[0], len_bytes[1]]) as usize;
    
    if len > MAX_STRING_LENGTH {
        return Err(AdsError::StringTooLarge { 
            length: len, 
            max: MAX_STRING_LENGTH 
        });
    }
    
    let str_bytes = self.read_bytes(len)?;
    String::from_utf8(str_bytes.to_vec())
        .map_err(|e| AdsError::InvalidStringEncoding(e.to_string()))
}
```

#### 1.3 Integer Overflow in Message Size Tracking

**Issue**: No tracking of total bytes consumed per message. Parser can be fed partial data repeatedly.

**Recommendation**:
```rust
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
            max: self.max_message_size 
        });
    }
    Ok(entry)
}
```

#### 1.4 FILETIME Timestamp Validation

**Location**: `parser.rs:145-166`

**Status**: ✅ Good - Correctly validates timestamp is after Unix epoch. No findings.

---

### 2. Binary Protocol Parser (Task #5) - HIGH PRIORITY

#### 2.1 UTF-8 Validation

**Issue**: UTF-8 validation happens AFTER allocation:
```rust
String::from_utf8(str_bytes.to_vec()).map_err(|e| {
    AdsError::InvalidStringEncoding(e.to_string())
})
```

**Risk**: Minimal (UTF-8 check is efficient), but should happen before allocation in ideal case.

**Recommendation**: Use `from_utf8_lossy()` for logging context fields (never fail parsing), or pre-validate:
```rust
// For message/logger fields: strict validation is OK
String::from_utf8(str_bytes.to_vec())?

// For context fields: use lossy conversion to avoid DoS via invalid UTF-8
String::from_utf8_lossy(str_bytes).to_string()
```

#### 2.2 Argument Index Bounds

**Issue**: Arguments indexed by u8 (0-255) without bounds checking:
```rust
if type_id == 1 {
    let index = reader.read_u8()?;
    let value = reader.read_value()?;
    arguments.insert(index as usize, value);  // Can insert up to 256 args
}
```

**Risk**: Memory exhaustion if attacker sends 256 large values repeatedly. Mitigated somewhat by max_queue_size config.

**Recommendation**:
```rust
const MAX_ARGUMENTS_PER_ENTRY: usize = 32;

if type_id == 1 {
    if arguments.len() >= MAX_ARGUMENTS_PER_ENTRY {
        return Err(AdsError::TooManyArguments);
    }
    let index = reader.read_u8()?;
    let value = reader.read_value()?;
    arguments.insert(index as usize, value);
}
```

#### 2.3 No Checksum/Integrity Validation

**Issue**: No frame-level integrity check (CRC, HMAC, length prefix).

**Note**: Original .NET implementation may not have this either. If protocol is not documented as having checksum, this may be acceptable for local/trusted networks.

**Recommendation**: Document protocol assumptions:
> ADS protocol operates on trusted local networks. Network-level integrity is not verified at application level; rely on TCP checksums and network security.

---

### 3. OTEL Exporter (Task #8) - CRITICAL

#### 3.1 No TLS Certificate Validation

**Issue**: OTEL exporter will make gRPC/HTTP calls to remote collectors without verifying certificates.

**Location**: Will be in `log4tc-otel` exporter implementation (currently not present).

**Risk**: CRITICAL - Man-in-the-middle attack can intercept logs and inject false logs. Credentials in custom headers could be stolen.

**Recommendation** (must implement):
```rust
use reqwest::Client;
use rustls::ClientConfig;

// For HTTP exporter:
let client = Client::builder()
    .https_only(true)  // Enforce HTTPS
    .danger_accept_invalid_certs(false)  // Validate certs
    .tls_version_min(rustls::TLS_VERSION_1_2)
    .build()?;

// For gRPC exporter (tonic):
let endpoint = Channel::from_static("https://collector:4317")
    .tls_config(ClientTlsConfig::new())?  // Validates by default
    .connect()
    .await?;
```

#### 3.2 Custom Headers Without Validation

**Config supports custom headers**:
```toml
[otel.headers]
"Authorization" = "Bearer token123"
"X-Custom-Header" = "value"
```

**Risk**: If token is stored in config file with readable permissions, it's exposed.

**Recommendation**:
```
- Document that auth tokens should NEVER be in config files
- Support environment variables: `Authorization = "$OTEL_AUTH_TOKEN"`
- Add validation that sensitive headers are not logged
```

---

### 4. Configuration Security (Task #3) - MEDIUM PRIORITY

#### 4.1 No File Permission Validation

**Issue**: Configuration file may contain secrets (auth tokens, API keys).

**Current**: `AppSettings::from_json_file()` just reads the file with current process permissions.

**Recommendation** (Windows-specific):
```rust
#[cfg(target_os = "windows")]
fn validate_config_file_permissions(path: &Path) -> Result<()> {
    use std::os::windows::fs::MetadataExt;
    let metadata = fs::metadata(path)?;
    
    // File should not be world-readable
    // Get ACLs and verify only admin/owner can read
    // This is platform-specific - use `windows` crate
    
    tracing::warn!("Config file accessible to multiple users - ensure secrets are rotated");
    Ok(())
}

pub fn from_json_file_secure(path: &Path) -> Result<Self> {
    validate_config_file_permissions(path)?;
    // ... load file ...
}
```

#### 4.2 Environment Variable Expansion Not Secure by Default

**Issue**: If implemented, must not allow:
- Code execution: `${cmd:whoami}`
- Path traversal: `${../../../etc/passwd}`

**Current Status**: Not implemented yet (good - defer to secure design).

**Recommendation** (for when implemented):
```rust
// Only allow explicit whitelist of safe vars
const SAFE_ENV_VARS: &[&str] = &[
    "OTEL_AUTH_TOKEN",
    "LOG_LEVEL",
    "HOSTNAME",
    // NOT: LOG4TC_CONFIG (avoid recursion)
];

fn expand_env_var(key: &str) -> Result<String> {
    if !SAFE_ENV_VARS.contains(&key) {
        return Err(Error::UnsafeEnvVar(key.to_string()));
    }
    std::env::var(key).ok_or_else(|| Error::EnvVarNotSet(key.to_string()))
}
```

---

### 5. Container Security (Docker Deployments) - MEDIUM PRIORITY

#### 5.1 Container Privileges

**Issue**: Running containers with unnecessary privileges increases attack surface.

**Recommendation**:
```dockerfile
# Dockerfile - run as non-root user
FROM debian:bookworm-slim
RUN useradd -m log4tc
USER log4tc
COPY --from=builder /build/target/release/log4tc-service /usr/local/bin/
ENTRYPOINT ["log4tc-service"]
```

#### 5.2 Docker Compose Security

**Recommendation**:
```yaml
services:
  log4tc:
    build: .
    # Do NOT use privileged: true
    security_opt:
      - no-new-privileges:true
    cap_drop:
      - ALL
    cap_add:
      - NET_BIND_SERVICE  # Only if needed for port binding
    read_only: true
    tmpfs:
      - /tmp
      - /var/tmp
```

#### 5.3 Configuration File Permissions (All Platforms)

**Unix/Linux**:
```bash
# Config file should not be world-readable (contains potential secrets)
chmod 600 /etc/log4tc/log4tc.toml
chown log4tc:log4tc /etc/log4tc/log4tc.toml
```

**Docker**:
```dockerfile
# Copy config with restrictive permissions
COPY --chown=log4tc:log4tc --chmod=600 config.toml /etc/log4tc/
```

---

## Dependency Security

### Current Cargo.toml Analysis

**High-Risk Dependencies**: None identified in initial review.

**Best Practice**: Run `cargo audit` regularly:
```bash
# Add to CI/CD:
cargo audit --deny warnings
```

**Key Dependencies to Monitor**:
- `tokio` - Async runtime, actively maintained
- `tonic` - gRPC, part of CNCF ecosystem
- `axum` - Web framework, maintained by Tokio team
- `opentelemetry` - OTEL SDK, CNCF supported

---

## OWASP Top 10 Alignment

| OWASP | Risk | Log4TC Status | Mitigation |
|-------|------|---------------|-----------|
| **A01:2021 - Broken Access Control** | Unauthed access to ADS | Port exposed | Firewall + bind loopback |
| **A02:2021 - Cryptographic Failures** | TLS not validated on OTEL export | CRITICAL | Implement TLS validation |
| **A03:2021 - Injection** | Log injection in output | LOW | Output encoding in formatters |
| **A04:2021 - Insecure Design** | No threat model in config | MEDIUM | Document assumptions |
| **A05:2021 - Security Misconfiguration** | Defaults allow 0.0.0.0 | MEDIUM | Secure defaults |
| **A06:2021 - Vulnerable Components** | Dependencies unaudited | LOW | Add `cargo audit` to CI |
| **A07:2021 - Authentication Failures** | No auth on OTEL endpoint | MEDIUM | Token validation recommended |
| **A08:2021 - Data Integrity Failures** | No message integrity | LOW | Document TCP reliability assumption |
| **A09:2021 - Logging Failures** | Sensitive data in logs | MEDIUM | Sanitize auth tokens before logging |
| **A10:2021 - SSRF** | Not applicable | N/A | N/A |

---

## Security Review Checklist

### Pre-Implementation (Task #4, #5, #8, #10)

- [ ] Code review focusing on memory safety (buffer overflows)
- [ ] Fuzz testing on binary parser with invalid inputs
- [ ] TLS certificate validation in OTEL exporter
- [ ] Configuration file permission validation (Windows)
- [ ] DOS mitigation (max message sizes, connection limits)

### Post-Implementation (Task #11 - Formal Security Review)

- [ ] Dependency audit (`cargo audit`)
- [ ] OWASP Top 10 checklist
- [ ] Privilege escalation testing (Windows service)
- [ ] Network exposure assessment
- [ ] Threat model validation against implementation
- [ ] Penetration testing recommendations

---

## Recommendations by Priority

### 🔴 CRITICAL (Block release)

1. **TLS Certificate Validation** (Task #8)
   - OTEL exporter must validate server certificates
   - Reject self-signed unless explicitly configured
   - Use TLS 1.2+

2. **Max Message Size Enforcement** (Task #5)
   - Parser must respect `max_message_size` config
   - Prevent memory exhaustion attacks

### 🟠 HIGH (Strongly recommended)

3. **Secure Configuration Defaults** (Task #3, #4)
   - Bind to localhost by default: `127.0.0.1:16150`
   - Document production network setup

4. **Privilege Validation** (Task #10)
   - Service runs as LOCAL SERVICE (not SYSTEM)
   - Config file has restricted ACLs

5. **String Length Limits** (Task #5)
   - Message, logger, task names: 64KB max each
   - Argument count: 32 per entry max

### 🟡 MEDIUM (Recommended in v1.1+)

6. **Configuration File Security** (Task #3)
   - Validate ACLs on Windows
   - Support environment variable injection safely

7. **Input Fuzzing** (Testing)
   - Add fuzzing tests for binary parser
   - Test with `cargo-fuzz`

8. **Dependency Auditing** (Task #15 - CI/CD)
   - `cargo audit` in build pipeline
   - Fail on security vulnerabilities

---

## Testing Strategy

### Security Test Cases

**Parser Robustness**:
```rust
#[test]
fn test_parser_max_string_length() {
    let mut data = vec![1u8]; // version
    data.extend_from_slice(&65537u16.to_le_bytes()); // Over 64KB
    assert!(AdsParser::parse(&data).is_err());
}

#[test]
fn test_parser_zero_length_strings() {
    // Should handle empty strings without panic
}

#[test]
fn test_parser_invalid_utf8() {
    // Should reject or sanitize invalid UTF-8
}
```

**Listener Security**:
```rust
#[tokio::test]
async fn test_connection_limit() {
    // Attempt 1000 connections, verify max_connections enforced
}

#[tokio::test]
async fn test_large_payload_rejected() {
    // Send 10MB payload, verify rejected before memory exhaustion
}
```

---

## References

- OWASP Top 10: https://owasp.org/www-project-top-ten/
- Secure Rust: https://anssi-fr.github.io/rust-guide/
- TLS Best Practices: https://wiki.mozilla.org/Security/Server_Side_TLS
- Docker Security Best Practices: https://docs.docker.com/engine/security/
- CIS Docker Benchmark: https://www.cisecurity.org/benchmark/docker

---

## Approval

- **Security Expert**: Ready for implementation feedback
- **Status**: Initial assessment complete, awaiting Task #4, #5, #8, #10 implementations
- **Next Review**: Upon completion of blocking tasks

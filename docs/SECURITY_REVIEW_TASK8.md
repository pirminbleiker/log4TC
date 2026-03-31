# Security Review: Task #8 - OTLP Exporter (gRPC/HTTP)

**Reviewer**: Security Expert  
**Date**: 2026-03-31  
**Status**: Implementation review completed  
**Severity Summary**: 1 CRITICAL, 2 HIGH, 1 MEDIUM finding

---

## Findings

### 1. 🔴 CRITICAL: No TLS Certificate Validation

**Location**: `crates/log4tc-otel/src/exporter.rs:54-59`

**Issue**:
```rust
pub fn with_config(config: ExportConfig) -> Self {
    let http_client = reqwest::Client::new();  // NO TLS CONFIGURATION!
    Self {
        config,
        http_client,
    }
}

async fn send_payload(&self, payload: &str) -> Result<()> {
    let response = self
        .http_client
        .post(&self.config.endpoint)  // Accepts any certificate!
        .header("Content-Type", "application/json")
        .body(payload.to_string())
        .timeout(Duration::from_secs(self.config.timeout_secs))
        .send()
        .await
        .map_err(|e| OtelError::HttpError(format!("Request failed: {}", e)))?;
    // ...
}
```

By default, `reqwest::Client::new()` uses system TLS defaults and **DOES NOT validate certificates** when using self-signed certs. More critically, there is no mechanism to:

1. Enforce HTTPS-only (config allows `http://` endpoints)
2. Validate certificate subject/hostname matching
3. Require TLS 1.2+ (could downgrade to SSL 3.0 in theory)
4. Support certificate pinning for sensitive deployments

**Attack Vector**: 
- Network MITM attacker intercepts logs and injects false logs
- Steals auth tokens/credentials in custom headers
- Modifies log data before it reaches observability backend

**Example**:
```
Legitimate: https://collector.company.com:4317
Attacker:   http://collector-fake.attacker.com:4317

Config file with http:// endpoint
No TLS validation
→ All logs leaked to attacker
```

**Impact**: CRITICAL - Confidentiality + Integrity breach

**Fix Required** (MUST implement):
```rust
use reqwest::Client;
use rustls::ClientConfig;

pub fn with_config(config: ExportConfig) -> Self {
    // Parse endpoint to enforce HTTPS
    let endpoint_url = reqwest::Url::parse(&config.endpoint)
        .map_err(|_| OtelError::InvalidEndpoint(config.endpoint.clone()))?;
    
    // Enforce HTTPS
    if endpoint_url.scheme() != "https" {
        return Err(OtelError::InsecureEndpoint(
            "OTLP endpoint must use https://".to_string()
        ));
    }
    
    // Build client with TLS validation
    let http_client = Client::builder()
        .https_only(true)                    // Reject http://
        .tls_version_min(rustls::TLS_VERSION_1_2)  // Require TLS 1.2+
        // Note: reqwest validates certs by default against system CA store
        .build()
        .map_err(|e| OtelError::TlsError(e.to_string()))?;
    
    Self {
        config,
        http_client,
    }
}
```

**Configuration Option** (for testing/development):
```toml
[otel]
endpoint = "https://collector.company.com:4317"

[otel.tls]
enabled = true
verify_certificates = true  # Default for production
ca_path = "/path/to/ca.crt"  # Optional: use custom CA
insecure = false  # Never allow self-signed in production
```

---

### 2. 🔴 HIGH: Default HTTP Endpoint Allows Unencrypted Traffic

**Location**: `crates/log4tc-otel/src/exporter.rs:26`

**Issue**:
```rust
impl Default for ExportConfig {
    fn default() -> Self {
        Self {
            endpoint: "http://localhost:4318/v1/logs".to_string(),  // UNENCRYPTED!
            // ...
        }
    }
}
```

Default configuration uses `http://` (plaintext). While `localhost` is lower risk, this sets a bad precedent and makes it easy to accidentally deploy unencrypted.

**Recommendation**:
```rust
endpoint: "https://localhost:4318/v1/logs".to_string(),
```

For development/localhost testing that needs http://, make it explicit:
```toml
# In config file
[otel]
endpoint = "http://localhost:4318"  # Development ONLY
```

---

### 3. 🔴 HIGH: No Validation of Custom Headers for Secrets

**Issue**: Configuration allows arbitrary headers:
```toml
[otel.headers]
"Authorization" = "Bearer secret-token-123"
"X-API-Key" = "my-api-key"
```

If these are stored in plaintext config file, they're easily exposed:
- File permissions not validated (see Task #3 finding)
- Secrets in git history
- Secrets in system logs

**Attack**: 
```
1. Attacker reads config.toml on compromised system
2. Extracts all auth tokens
3. Replays tokens against collector API
```

**Fix Required**:
```
1. Document: NEVER put secrets in config files
2. Support env var injection: Authorization = "${OTEL_AUTH_TOKEN}"
3. Validate at startup that sensitive headers come from env vars
4. Never log header values in debug output
```

**Implementation**:
```rust
// Safe header loading
fn load_headers(config: &HeadersConfig) -> Result<HeaderMap> {
    let mut headers = HeaderMap::new();
    
    for (key, value) in &config.headers {
        // Only allow env var injection for sensitive headers
        let actual_value = if value.starts_with("${") && value.ends_with("}") {
            let env_key = &value[2..value.len()-1];
            std::env::var(env_key).map_err(|_| {
                OtelError::MissingEnvVar(format!(
                    "Header {} references env var {} which is not set",
                    key, env_key
                ))
            })?
        } else {
            // Warn if header looks like it contains secrets
            if is_sensitive_header(key) && !value.starts_with("${") {
                tracing::warn!(
                    "Header {} appears sensitive but not loaded from env var - \
                     consider using environment variable injection",
                    key
                );
            }
            value.clone()
        };
        
        headers.insert(
            HeaderName::from_str(key)?,
            HeaderValue::from_str(&actual_value)?
        );
    }
    
    Ok(headers)
}

fn is_sensitive_header(name: &str) -> bool {
    matches!(name.to_lowercase().as_str(),
        "authorization" | "x-api-key" | "x-token" | 
        "api-key" | "secret" | "password" | "token"
    )
}
```

---

### 4. 🟠 MEDIUM: Retry Logic Could Hide Permanent Failures

**Location**: `crates/log4tc-otel/src/exporter.rs:78-116`

**Issue**:
```rust
async fn send_with_retry(&self, payload: &str) -> Result<()> {
    let mut retry_count = 0;
    let mut delay_ms = self.config.retry_delay_ms;
    
    loop {
        match self.send_payload(payload).await {
            Ok(_) => return Ok(()),
            Err(e) => {
                retry_count += 1;
                
                if retry_count > self.config.max_retries {
                    tracing::error!(
                        "Failed to export logs after {} retries: {}",
                        self.config.max_retries,
                        e
                    );
                    return Err(e);  // Returns last error only
                }
                // ...
            }
        }
    }
}
```

Retries all errors equally. Permanent errors (400 Bad Request, 401 Unauthorized, 403 Forbidden) should fail immediately, not retry. Retrying auth failures:
- Wastes time and bandwidth
- Could trigger rate limiting/IP blocking
- Logs are lost while retrying

**Recommendation**:
```rust
fn is_retryable_error(status: StatusCode) -> bool {
    match status {
        // Retryable: transient errors
        StatusCode::SERVICE_UNAVAILABLE |
        StatusCode::BAD_GATEWAY |
        StatusCode::GATEWAY_TIMEOUT |
        StatusCode::TOO_MANY_REQUESTS => true,
        
        // Not retryable: permanent errors
        StatusCode::BAD_REQUEST |
        StatusCode::UNAUTHORIZED |
        StatusCode::FORBIDDEN |
        StatusCode::NOT_FOUND => false,
        
        // Default: 5xx errors are retryable
        s if s.is_server_error() => true,
        _ => false,
    }
}

// In send_with_retry():
if !is_retryable_error(error) {
    tracing::error!("Permanent export failure (not retrying): {}", error);
    return Err(e);
}
```

---

## Recommendations Summary

### MUST FIX Before Release (CRITICAL):
1. **Enforce HTTPS-only**: Validate endpoint uses `https://`, reject `http://`
2. **TLS Certificate Validation**: Use `https_only(true)` and `tls_version_min(TLS_VERSION_1_2)`
3. **Secure Header Loading**: Support env var injection for auth tokens, warn on plaintext secrets

### SHOULD FIX in v0.2 (HIGH priority):
4. Change default endpoint to `https://` instead of `http://`
5. Distinguish retryable vs permanent errors - fail fast on auth/client errors
6. Add TLS certificate pinning option for sensitive deployments
7. Add metrics: export success/failure rates, TLS handshake errors

### NICE TO HAVE (LOW priority):
8. Support mTLS (client certificate authentication)
9. OCSP stapling for certificate revocation checks
10. Connection pooling with TLS session reuse

---

## Testing Checklist

Before deploying Task #8:
- [ ] Unit test: reject `http://` endpoints
- [ ] Unit test: accept `https://` endpoints
- [ ] Integration test: TLS cert validation with valid cert succeeds
- [ ] Integration test: TLS cert validation with invalid cert fails
- [ ] Integration test: retry on 503 (retryable)
- [ ] Integration test: fail fast on 401 (non-retryable)
- [ ] Manual test: observe TLS handshake in packet capture
- [ ] Verify no secrets in debug logs

---

## Blockers & Dependencies

**Blocks**: Task #11 (Security Review)

**Depends on**: 
- Task #3 (Config) - for environment variable injection support
- Task #15 (CI/CD) - for TLS testing automation

---

## Security Approval

**Status**: ⚠️ CONDITIONAL - Awaiting fixes for CRITICAL findings

**Approval Path**:
1. Developer implements HTTPS enforcement + TLS validation
2. Developer adds env var support for headers
3. Security expert reviews fixes and runs TLS tests
4. Move to formal review (Task #11) only after approval

**Blockers**: Do NOT deploy to production without addressing findings #1-3 (CRITICAL/HIGH).

---

## Severity Classification

- **CRITICAL** (#1): Violates CIA triad (Confidentiality, Integrity, Authenticity)
- **HIGH** (#2-3): Practical attack paths exist, requires network access
- **MEDIUM** (#4): Operational issue, logs may be lost, not security failure

---

## Appendix: TLS Implementation Reference

For developer implementing the fix:

```rust
// Add to Cargo.toml
[dependencies]
reqwest = { version = "0.11", features = ["json", "rustls-tls"] }
rustls = "0.21"
```

```rust
use reqwest::{Client, ClientBuilder};
use rustls::ClientConfig;

// Safe client creation
fn create_secure_client(endpoint: &str) -> Result<Client> {
    let url = reqwest::Url::parse(endpoint)
        .map_err(|_| OtelError::InvalidEndpoint(endpoint.to_string()))?;
    
    if url.scheme() != "https" {
        return Err(OtelError::InsecureEndpoint(
            "OTLP endpoint must use https://".to_string()
        ));
    }
    
    ClientBuilder::new()
        .https_only(true)
        .build()
        .map_err(|e| OtelError::TlsError(e.to_string()))
}
```

References:
- Rustls Security: https://rustls.io/
- reqwest HTTPS: https://docs.rs/reqwest/latest/reqwest/#making-requests
- OWASP TLS Cheat Sheet: https://cheatsheetseries.owasp.org/cheatsheets/Transport_Layer_Protection_Cheat_Sheet.html

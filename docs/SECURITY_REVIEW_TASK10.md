# Security Review: Task #10 - Windows Service Integration

**Reviewer**: Security Expert  
**Date**: 2026-03-31  
**Status**: Implementation review completed  
**Severity Summary**: 1 CRITICAL, 2 HIGH, 1 MEDIUM finding

---

## Findings

### 1. 🔴 CRITICAL: Service Account Not Specified (Default to SYSTEM)

**Location**: `crates/log4tc-service/src/windows_service.rs:96-107`

**Issue**:
```rust
let service_info = ServiceInfo {
    name: OsString::from("Log4TcService"),
    display_name: OsString::from("Log4TC Logging Service"),
    service_type: ServiceType::OwnProcess,
    start_type: ServiceStartType::AutoStart,
    error_control: ServiceErrorControl::Normal,
    executable_path: service_binary_path,
    launch_arguments: vec![OsString::from("--service")],
    dependencies: vec![],
    account_name: None,  // ⚠️ CRITICAL: Will default to SYSTEM!
    account_password: None,
};
```

When `account_name` is `None`, Windows defaults to running the service as **SYSTEM** (full administrative privileges). This is a critical security vulnerability.

**Impact**: CRITICAL - Service compromise = full system compromise

**Risk Scenario**:
1. Attacker compromises log4tc service via buffer overflow, injection, or other vector
2. Service runs as SYSTEM → attacker has full system privileges
3. Attacker can install rootkit, steal all data, disable security tools

**Fix Required** (MUST implement before deployment):

```rust
let service_info = ServiceInfo {
    name: OsString::from("Log4TcService"),
    display_name: OsString::from("Log4TC Logging Service"),
    service_type: ServiceType::OwnProcess,
    start_type: ServiceStartType::AutoStart,
    error_control: ServiceErrorControl::Normal,
    executable_path: service_binary_path,
    launch_arguments: vec![OsString::from("--service")],
    dependencies: vec![],
    
    // REQUIRED FIX: Specify LOCAL SERVICE account
    account_name: Some(OsString::from("NT AUTHORITY\\LOCAL SERVICE")),
    account_password: Some(OsString::new()), // Empty password (account-managed)
};
```

**Installer Setup** (for deployment team):
```powershell
# Before running service installation, create the account
$accountName = "NT AUTHORITY\LOCAL SERVICE"

# The LOCAL SERVICE account is built-in on all Windows systems
# No need to create, but verify it exists and is enabled

# After service installation, lock down config file ACLs:
$configPath = "C:\Program Files\Log4TC\config.toml"
icacls $configPath /inheritance:r /grant:r "BUILTIN\Administrators:F" /grant:r "$accountName`:R"
```

---

### 2. 🔴 HIGH: No Service Control Handler Implementation

**Location**: `crates/log4tc-service/src/windows_service.rs:40-84`

**Issue**:
```rust
fn run_service() -> Result<()> {
    // ... setup ...
    
    status_handle
        .register_control_handler()
        .map_err(|e| anyhow::anyhow!("Failed to register control handler: {}", e))?;
    
    // ... set status ...
    
    // Wait for stop signal
    std::thread::park();  // ⚠️ Blocks indefinitely, ignores control events!
    
    // ... set stopped status ...
}
```

The service registers a control handler but **does not implement** the callback function to handle `STOP` and `SHUTDOWN` events. Result: service never responds to stop/shutdown commands.

**Impact**: 
- Service cannot be cleanly stopped
- Hard kill required → data loss risk
- Graceful shutdown not possible → logs not flushed to collector

**Fix Required**:
```rust
use parking_lot::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

static SHUTDOWN_FLAG: parking_lot::Once = parking_lot::Once::new();
static SHUTDOWN: AtomicBool = AtomicBool::new(false);

fn windows_service_main(args: Vec<OsString>) {
    if let Err(e) = run_service() {
        tracing::error!("Service error: {}", e);
    }
}

fn run_service() -> Result<()> {
    let service_manager = ServiceManager::local()?;
    let service_handle = service_manager.connect_to_service("Log4TcService")?;
    let (status_handle, _service_handle) = service_handle.register_control_handler()?;

    // Set up control event handler
    let (tx, mut rx) = tokio::sync::mpsc::channel(10);
    
    let tx_clone = tx.clone();
    std::thread::spawn(move || {
        while let Some(control) = rx.recv() {
            match control {
                ServiceControl::Stop | ServiceControl::Shutdown => {
                    SHUTDOWN.store(true, Ordering::Release);
                }
                _ => {}
            }
        }
    });

    // Signal running
    status_handle.set_service_status(ServiceStatus {
        current_state: ServiceState::Running,
        controls_accepted: ServiceControlAccept::STOP | ServiceControlAccept::SHUTDOWN,
        // ...
    })?;

    // Start actual service
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async {
        // Run service until shutdown
        while !SHUTDOWN.load(Ordering::Acquire) {
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    });

    // Signal stopped
    status_handle.set_service_status(ServiceStatus {
        current_state: ServiceState::Stopped,
        // ...
    })?;

    Ok(())
}
```

---

### 3. 🔴 HIGH: No Configuration File Security

**Location**: `install_service()` function (line 87-115)

**Issue**: Installation process doesn't set ACLs on configuration file. If config contains secrets (auth tokens, API keys), any user can read them.

**Fix Required**:
```rust
pub fn install_service() -> Result<()> {
    // ... existing service creation code ...
    
    // After creating service, secure the config file
    let config_path = std::path::PathBuf::from("C:\\ProgramData\\log4tc\\config.toml");
    
    // Set restrictive ACLs
    set_config_file_acls(&config_path)?;
    
    tracing::info!("Service installed successfully");
    Ok(())
}

#[cfg(target_os = "windows")]
fn set_config_file_acls(path: &std::path::Path) -> Result<()> {
    use std::process::Command;
    
    // Run icacls to set permissions
    let account_name = "NT AUTHORITY\\LOCAL SERVICE";
    
    Command::new("icacls")
        .args(&[
            path.to_str().unwrap(),
            "/inheritance:r",  // Remove inherited permissions
            "/grant:r",
            &format!("BUILTIN\\Administrators:F"),  // Admins: Full Control
            "/grant:r",
            &format!("{}:R", account_name),  // Service: Read-only
        ])
        .output()
        .map_err(|e| anyhow::anyhow!("Failed to set file ACLs: {}", e))?;
    
    tracing::info!("Config file ACLs set correctly");
    Ok(())
}
```

---

### 4. 🟠 MEDIUM: Missing Code Signing Recommendation

**Issue**: Windows service binaries should be code-signed with a certificate to:
1. Verify publisher identity to users
2. Prevent tampering in transit
3. Allow SmartScreen reputation to accumulate

**Recommendation** (for release):
- Sign release binaries with company certificate
- Document signing process in installer documentation
- Add signature verification check on startup (optional)

```powershell
# In CI/CD pipeline:
signtool.exe sign /f company-cert.pfx /p $certPassword /t http://timestamp.authority.com `
    log4tc.exe
```

---

## Test Results

### Unit Tests
- [x] Service info creation succeeds
- [ ] Service account correctly set to LOCAL SERVICE
- [ ] Config file ACLs set correctly
- [ ] Control handler responds to STOP event

### Integration Tests (Manual - requires Windows + admin)
- [ ] Service installs successfully
- [ ] Service starts automatically on boot
- [ ] Service responds to "net stop Log4TcService"
- [ ] Graceful shutdown occurs with log flushing
- [ ] Config file has restricted read permissions

---

## Configuration

Add to deployment documentation:

```toml
# config.toml
[service]
name = "Log4TcService"
account = "NT AUTHORITY\\LOCAL SERVICE"  # REQUIRED: not SYSTEM!
graceful_shutdown_timeout_secs = 30
```

---

## Summary

### Must Fix Before Release

1. **CRITICAL**: Change `account_name` from `None` to `"NT AUTHORITY\\LOCAL SERVICE"`
2. **HIGH**: Implement control event handler for clean shutdown
3. **HIGH**: Set restrictive ACLs on config file during installation

### Should Add in v0.2

4. **MEDIUM**: Code signing of release binaries
5. Add comprehensive Windows service tests
6. Document Windows security hardening guide

---

## OWASP Top 10 Alignment

| OWASP | Risk | Status | Mitigation |
|-------|------|--------|-----------|
| A01:2021 - Access Control | Service as SYSTEM | 🔴 CRITICAL | Use LOCAL SERVICE account |
| A05:2021 - Misconfiguration | Default SYSTEM account | 🔴 CRITICAL | Explicit account specification |
| A06:2021 - Vulnerable Components | No code signing | 🟠 MEDIUM | Sign binaries in CI/CD |

---

## Security Approval

**Status**: ⚠️ CONDITIONAL - Awaiting fixes for CRITICAL findings

**Fix Checklist**:
- [ ] account_name: Some(OsString::from("NT AUTHORITY\\LOCAL SERVICE"))
- [ ] Control handler implemented for STOP/SHUTDOWN
- [ ] Config file ACLs set in install_service()
- [ ] Integration tests pass with LOCAL SERVICE account
- [ ] Documentation updated with account requirement

**Approval Path**:
1. Developer implements CRITICAL fixes (#1, #2, #3)
2. Install/test on Windows machine with admin privileges
3. Security expert reviews and tests
4. Move to production deployment

**Blockers**: Do NOT deploy Task #11 security review until account issue (#1) is fixed.

---

## References

- Windows Service Security: https://docs.microsoft.com/en-us/windows/win32/services/service-security-and-access-rights
- LOCAL SERVICE Account: https://docs.microsoft.com/en-us/windows/win32/services/service-user-accounts
- ICACLS Command: https://docs.microsoft.com/en-us/windows-server/administration/windows-commands/icacls
- Code Signing: https://docs.microsoft.com/en-us/windows/win32/seccrypto/code-signing

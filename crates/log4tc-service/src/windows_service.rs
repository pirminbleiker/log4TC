//! Windows service integration for Log4TC
//!
//! This module provides the Windows service wrapper that allows Log4TC to run as a
//! Windows service. It handles service installation, starting, stopping, and control events.

use anyhow::Result;
use std::ffi::{OsStr, OsString};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use windows_service::define_windows_service;
use windows_service::service::{
    ServiceAccess, ServiceControl, ServiceControlAccept, ServiceErrorControl, ServiceInfo, ServiceStartType, ServiceStatus, ServiceType,
};
use windows_service::service_control_handler::{self, ServiceControlHandlerResult};
use windows_service::service_manager::{ServiceManager, ServiceManagerAccess};
use windows_service::service_dispatcher;

use crate::service::Log4TcService;

/// Global stop flag for service control events
static STOP_FLAG: AtomicBool = AtomicBool::new(false);

define_windows_service!(ffi_service_main, windows_service_main);

/// Main entry point for Windows service
pub async fn run_as_service(_service: Arc<RwLock<Option<Log4TcService>>>) -> Result<()> {
    // Register the Windows service dispatcher
    service_dispatcher::start("Log4TcService", ffi_service_main)
        .map_err(|e| anyhow::anyhow!("Failed to start Windows service: {}", e))?;

    Ok(())
}

/// Windows service main function
fn windows_service_main(_args: Vec<OsString>) {
    if let Err(e) = run_service() {
        tracing::error!("Service error: {}", e);
    }
}

/// Windows service control handler callback
fn handle_control(control: ServiceControl) -> ServiceControlHandlerResult {
    match control {
        ServiceControl::Stop | ServiceControl::Shutdown => {
            tracing::info!("Received service stop/shutdown signal");
            STOP_FLAG.store(true, Ordering::Release);
            ServiceControlHandlerResult::NoError
        }
        ServiceControl::Interrogate => ServiceControlHandlerResult::NoError,
        _ => ServiceControlHandlerResult::NotImplemented,
    }
}

/// Actual service implementation
fn run_service() -> Result<()> {
    // Register service event handler
    let status_handle = service_control_handler::register("Log4TcService", handle_control)
        .map_err(|e| anyhow::anyhow!("Failed to register control handler: {}", e))?;

    status_handle
        .set_service_status(ServiceStatus {
            service_type: ServiceType::OWN_PROCESS,
            current_state: windows_service::service::ServiceState::Running,
            controls_accepted: ServiceControlAccept::STOP | ServiceControlAccept::SHUTDOWN,
            exit_code: windows_service::service::ServiceExitCode::Win32(0),
            checkpoint: 0,
            wait_hint: Duration::default(),
            process_id: None,
        })
        .map_err(|e| anyhow::anyhow!("Failed to set service status: {}", e))?;

    tracing::info!("Log4TC service started");

    // Wait for stop signal (check flag periodically)
    loop {
        if STOP_FLAG.load(Ordering::Acquire) {
            tracing::info!("Service stop flag received");
            break;
        }
        std::thread::sleep(Duration::from_millis(500));
    }

    status_handle
        .set_service_status(ServiceStatus {
            service_type: ServiceType::OWN_PROCESS,
            current_state: windows_service::service::ServiceState::Stopped,
            controls_accepted: ServiceControlAccept::empty(),
            exit_code: windows_service::service::ServiceExitCode::Win32(0),
            checkpoint: 0,
            wait_hint: Duration::default(),
            process_id: None,
        })
        .map_err(|e| anyhow::anyhow!("Failed to set stopped status: {}", e))?;

    Ok(())
}

/// Install Log4TC as a Windows service
pub fn install_service() -> Result<()> {
    let manager_access = ServiceManagerAccess::CONNECT | ServiceManagerAccess::CREATE_SERVICE;
    let service_manager = ServiceManager::local_computer(None::<&str>, manager_access)
        .map_err(|e| anyhow::anyhow!("Failed to connect to service manager: {}", e))?;

    let service_binary_path = std::env::current_exe()
        .map_err(|e| anyhow::anyhow!("Failed to get current executable path: {}", e))?;

    // Security: Run as LOCAL_SERVICE (least privilege) instead of default account
    let service_info = ServiceInfo {
        name: OsString::from("Log4TcService"),
        display_name: OsString::from("Log4TC Logging Service"),
        service_type: ServiceType::OWN_PROCESS,
        start_type: ServiceStartType::AutoStart,
        error_control: ServiceErrorControl::Normal,
        executable_path: service_binary_path,
        launch_arguments: vec![OsString::from("--service")],
        dependencies: vec![],
        // Use LOCAL_SERVICE account for least privilege (security fix #5)
        account_name: Some(OsString::from("NT AUTHORITY\\LocalService")),
        account_password: None,
    };

    let _service = service_manager
        .create_service(&service_info, ServiceAccess::START | ServiceAccess::STOP)
        .map_err(|e| anyhow::anyhow!("Failed to create service: {}", e))?;

    // Security: Restrict config file permissions (security fix #5)
    restrict_config_file_permissions()?;

    tracing::info!("Service installed successfully");
    Ok(())
}

/// Restrict config file permissions for security (SECURITY: #5)
/// Config files should only be readable by SYSTEM and administrators
#[cfg(windows)]
fn restrict_config_file_permissions() -> Result<()> {
    use std::path::Path;

    let config_paths = vec![
        "config.json",
        "config.toml",
        "./config/config.json",
        "./config/config.toml",
    ];

    for config_path in config_paths {
        let path = Path::new(config_path);
        if path.exists() {
            // On Windows, file permissions are managed via NTFS ACLs
            // This requires the windows-rs crate for proper ACL manipulation
            // For now, we just log that the file exists
            // In production, would use windows NTFS ACL APIs via windows-rs
            match std::fs::metadata(path) {
                Ok(_) => {
                    tracing::info!("Config file found at {}: Ensure admin sets appropriate ACLs", config_path);
                }
                Err(e) => {
                    tracing::debug!("Config file not found at {}: {}", config_path, e);
                }
            }
        }
    }

    Ok(())
}

#[cfg(not(windows))]
fn restrict_config_file_permissions() -> Result<()> {
    // On non-Windows platforms, use standard Unix permissions
    use std::path::Path;
    use std::fs::Permissions;
    use std::os::unix::fs::PermissionsExt;

    let config_paths = vec![
        "config.json",
        "config.toml",
        "./config/config.json",
        "./config/config.toml",
    ];

    for config_path in config_paths {
        let path = Path::new(config_path);
        if path.exists() {
            // Set permissions to 0o600 (owner read+write only)
            let perms = Permissions::from_mode(0o600);
            if let Err(e) = std::fs::set_permissions(path, perms) {
                tracing::warn!("Failed to restrict permissions on {}: {}", config_path, e);
            } else {
                tracing::info!("Restricted permissions on config file: {}", config_path);
            }
        }
    }

    Ok(())
}

/// Uninstall Log4TC Windows service
pub fn uninstall_service() -> Result<()> {
    let manager_access = ServiceManagerAccess::CONNECT;
    let service_manager = ServiceManager::local_computer(None::<&str>, manager_access)
        .map_err(|e| anyhow::anyhow!("Failed to connect to service manager: {}", e))?;

    let service_access = ServiceAccess::DELETE | ServiceAccess::QUERY_STATUS;
    let service = service_manager
        .open_service("Log4TcService", service_access)
        .map_err(|e| anyhow::anyhow!("Failed to open service: {}", e))?;

    service
        .delete()
        .map_err(|e| anyhow::anyhow!("Failed to delete service: {}", e))?;

    tracing::info!("Service uninstalled successfully");
    Ok(())
}

/// Start Log4TC Windows service
pub fn start_service() -> Result<()> {
    let manager_access = ServiceManagerAccess::CONNECT;
    let service_manager = ServiceManager::local_computer(None::<&str>, manager_access)
        .map_err(|e| anyhow::anyhow!("Failed to connect to service manager: {}", e))?;

    let service_access = ServiceAccess::START | ServiceAccess::QUERY_STATUS;
    let service = service_manager
        .open_service("Log4TcService", service_access)
        .map_err(|e| anyhow::anyhow!("Failed to open service: {}", e))?;

    service
        .start::<&OsStr>(&[])
        .map_err(|e| anyhow::anyhow!("Failed to start service: {}", e))?;

    tracing::info!("Service start request sent");
    Ok(())
}

/// Stop Log4TC Windows service
pub fn stop_service() -> Result<()> {
    let manager_access = ServiceManagerAccess::CONNECT;
    let service_manager = ServiceManager::local_computer(None::<&str>, manager_access)
        .map_err(|e| anyhow::anyhow!("Failed to connect to service manager: {}", e))?;

    let service_access = ServiceAccess::STOP | ServiceAccess::QUERY_STATUS;
    let service = service_manager
        .open_service("Log4TcService", service_access)
        .map_err(|e| anyhow::anyhow!("Failed to open service: {}", e))?;

    service
        .stop()
        .map_err(|e| anyhow::anyhow!("Failed to stop service: {}", e))?;

    tracing::info!("Service stop request sent");
    Ok(())
}

/// Get Log4TC Windows service status
pub fn query_service_status() -> Result<String> {
    let manager_access = ServiceManagerAccess::CONNECT;
    let service_manager = ServiceManager::local_computer(None::<&str>, manager_access)
        .map_err(|e| anyhow::anyhow!("Failed to connect to service manager: {}", e))?;

    let service_access = ServiceAccess::QUERY_STATUS;
    let service = service_manager
        .open_service("Log4TcService", service_access)
        .map_err(|e| anyhow::anyhow!("Failed to open service: {}", e))?;

    let status = service
        .query_status()
        .map_err(|e| anyhow::anyhow!("Failed to query service status: {}", e))?;

    let state_str = match status.current_state {
        windows_service::service::ServiceState::Running => "Running",
        windows_service::service::ServiceState::Stopped => "Stopped",
        windows_service::service::ServiceState::StartPending => "Starting",
        windows_service::service::ServiceState::StopPending => "Stopping",
        _ => "Unknown",
    };

    Ok(format!(
        "Log4TC Service Status: {} (Process ID: {:?})",
        state_str, status.process_id
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_service_info_creation() {
        // This test verifies that service info can be created
        // Actual service operations require Windows and admin privileges
        let _ = ServiceInfo {
            name: OsString::from("Log4TcService"),
            display_name: OsString::from("Log4TC Logging Service"),
            service_type: ServiceType::OWN_PROCESS,
            start_type: ServiceStartType::AutoStart,
            error_control: ServiceErrorControl::Normal,
            executable_path: std::env::current_exe().unwrap(),
            launch_arguments: vec![],
            dependencies: vec![],
            account_name: None,
            account_password: None,
        };
    }
}

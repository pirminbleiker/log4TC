//! Log4TC Service - Main entry point
//!
//! This is the main Log4TC service that receives logs from TwinCAT PLCs
//! via OpenTelemetry protocol and dispatches them to configured outputs.
//!
//! Supports running as a Windows service or standalone application.

use anyhow::{Context, Result};
use log4tc_core::AppSettings;
use std::path::PathBuf;
use tracing_subscriber;

mod dispatcher;
mod service;

#[cfg(windows)]
mod windows_service;

use service::Log4TcService;

#[tokio::main]
async fn main() -> Result<()> {
    // Parse command line arguments
    let args: Vec<String> = std::env::args().collect();

    // Initialize logging first (before any other operations)
    init_logging()?;

    // Handle Windows service-specific commands
    #[cfg(windows)]
    if args.len() > 1 {
        match args[1].as_str() {
            "install" => {
                tracing::info!("Installing Log4TC as Windows service");
                windows_service::install_service()?;
                return Ok(());
            }
            "uninstall" => {
                tracing::info!("Uninstalling Log4TC Windows service");
                windows_service::uninstall_service()?;
                return Ok(());
            }
            "start" => {
                tracing::info!("Starting Log4TC Windows service");
                windows_service::start_service()?;
                return Ok(());
            }
            "stop" => {
                tracing::info!("Stopping Log4TC Windows service");
                windows_service::stop_service()?;
                return Ok(());
            }
            "status" => {
                let status = windows_service::query_service_status()?;
                println!("{}", status);
                return Ok(());
            }
            "service" => {
                // Running as Windows service
                tracing::info!("Starting Log4TC Service (Windows service mode)");
                run_service().await?;
                return Ok(());
            }
            _ => {
                eprintln!(
                    "Unknown argument: {}. Valid commands: install|uninstall|start|stop|status|service",
                    args[1]
                );
                return Ok(());
            }
        }
    }

    // Default: run as console application
    tracing::info!("Starting Log4TC Service (console mode)");
    run_service().await?;

    tracing::info!("Log4TC Service stopped");
    Ok(())
}

/// Run the Log4TC service
async fn run_service() -> Result<()> {
    // Load configuration
    let config_path = std::env::var("LOG4TC_CONFIG")
        .unwrap_or_else(|_| "config.json".to_string());

    let settings = AppSettings::from_json_file(&PathBuf::from(&config_path))
        .context("Failed to load configuration")?;

    tracing::info!("Configuration loaded from {}", config_path);

    // Create and run service
    let service = Log4TcService::new(settings).await?;
    service.run().await?;

    Ok(())
}

fn init_logging() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("log4tc_service=info".parse()?),
        )
        .with_writer(std::io::stderr)
        .init();

    Ok(())
}

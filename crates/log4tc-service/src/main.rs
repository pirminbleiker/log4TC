//! Log4TC Service - Main entry point
//!
//! This is the main Log4TC service that receives logs from TwinCAT PLCs
//! via OpenTelemetry protocol and dispatches them to configured outputs.
//!
//! A cross-platform standalone binary that can run on Linux, macOS, and Windows.

use anyhow::{Context, Result};
use clap::Parser;
use log4tc_core::AppSettings;
use std::path::PathBuf;
use tracing_subscriber;

mod dispatcher;
mod service;

use service::Log4TcService;

#[derive(Parser, Debug)]
#[command(name = "log4tc-service")]
#[command(about = "Log4TC logging service - bridge for Beckhoff TwinCAT and OpenTelemetry")]
#[command(version)]
struct Args {
    /// Path to configuration file
    #[arg(short, long, default_value = "config.toml")]
    config: PathBuf,

    /// ADS listener port
    #[arg(short, long, default_value = "16150")]
    ads_port: u16,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Parse command line arguments
    let args = Args::parse();

    // Initialize logging first (before any other operations)
    init_logging()?;

    tracing::info!("Starting Log4TC Service");
    run_service(args).await?;

    tracing::info!("Log4TC Service stopped");
    Ok(())
}

/// Run the Log4TC service
async fn run_service(args: Args) -> Result<()> {
    // Load configuration
    let settings = AppSettings::from_json_file(&args.config)
        .context(format!("Failed to load configuration from {}", args.config.display()))?;

    tracing::info!("Configuration loaded from {}", args.config.display());
    tracing::info!("ADS listener will bind to port {}", args.ads_port);

    // Create and run service
    let service = Log4TcService::new(settings, args.ads_port).await?;
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

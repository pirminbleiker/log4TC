//! Log4TC Service - Standalone logging bridge for Beckhoff TwinCAT PLCs
//!
//! Receives ADS log messages via AMS/TCP (port 48898) and exports them
//! to a configured endpoint (e.g. Victoria-Logs, OTEL Collector).

use anyhow::{Context, Result};
use clap::Parser;
use log4tc_core::AppSettings;
use std::path::PathBuf;

mod dispatcher;
mod service;

use service::Log4TcService;

#[derive(Parser, Debug)]
#[command(name = "log4tc-service")]
#[command(about = "Log4TC - TwinCAT PLC logging to Victoria-Logs/OTEL")]
#[command(version)]
struct Args {
    /// Path to configuration file (JSON)
    #[arg(short, long, default_value = "config.json")]
    config: PathBuf,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("log4tc_service=info".parse()?),
        )
        .with_writer(std::io::stderr)
        .init();

    let settings = AppSettings::from_json_file(&args.config)
        .context(format!("Failed to load config from {}", args.config.display()))?;

    tracing::info!(
        "Log4TC starting: AMS/TCP :{} (Net ID {}), export → {}",
        settings.receiver.ams_tcp_port,
        settings.receiver.ams_net_id,
        settings.export.endpoint,
    );

    let service = Log4TcService::new(settings).await?;
    service.run().await?;

    Ok(())
}

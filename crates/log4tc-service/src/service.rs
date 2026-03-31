//! Main service orchestration with graceful shutdown and backpressure handling

use anyhow::Result;
use log4tc_ads::AdsListener;
use log4tc_core::AppSettings;
use log4tc_otel::OtelHttpReceiver;
use std::time::Duration;
use tokio::sync::{mpsc, broadcast};
use tokio::time::timeout;

use crate::dispatcher::LogDispatcher;

/// Main Log4TC Service
pub struct Log4TcService {
    settings: AppSettings,
    log_dispatcher: LogDispatcher,
    ads_port: u16,
}

impl Log4TcService {
    /// Create a new Log4TC service instance
    pub async fn new(settings: AppSettings, ads_port: u16) -> Result<Self> {
        let dispatcher = LogDispatcher::new(&settings).await?;

        Ok(Self {
            settings,
            log_dispatcher: dispatcher,
            ads_port,
        })
    }

    /// Run the service
    pub async fn run(&self) -> Result<()> {
        tracing::info!("Log4TC Service starting receivers and dispatcher");

        // Create channel for log entries with bounded capacity
        let (log_tx, mut log_rx) = mpsc::channel(self.settings.service.channel_capacity);

        // Create shutdown signal channel (broadcast for multiple receivers)
        let (shutdown_tx, mut shutdown_rx) = broadcast::channel(1);

        // Start ADS listener (legacy protocol support)
        let ads_listener = AdsListener::new(
            "127.0.0.1".to_string(),
            self.ads_port,
            log_tx.clone(),
        );

        let mut shutdown_rx_ads = shutdown_tx.subscribe();
        let ads_handle = tokio::spawn(async move {
            tokio::select! {
                result = ads_listener.start() => {
                    if let Err(e) = result {
                        tracing::error!("ADS listener error: {}", e);
                    }
                }
                _ = shutdown_rx_ads.recv() => {
                    tracing::info!("ADS listener shutdown requested");
                }
            }
        });

        // Start OTEL HTTP receiver
        let receiver = OtelHttpReceiver::new(
            self.settings.receiver.host.clone(),
            self.settings.receiver.http_port,
            log_tx.clone(),
        );

        let mut shutdown_rx_otel = shutdown_tx.subscribe();
        let receiver_handle = tokio::spawn(async move {
            tokio::select! {
                result = receiver.start() => {
                    if let Err(e) = result {
                        tracing::error!("OTEL HTTP receiver error: {}", e);
                    }
                }
                _ = shutdown_rx_otel.recv() => {
                    tracing::info!("OTEL HTTP receiver shutdown requested");
                }
            }
        });

        // Start dispatcher task with backpressure handling
        let dispatcher = self.log_dispatcher.clone();
        let dispatcher_handle = tokio::spawn(async move {
            let mut processed = 0u64;
            let mut dropped = 0u64;

            loop {
                tokio::select! {
                    Some(entry) = log_rx.recv() => {
                        if let Err(e) = dispatcher.dispatch(entry).await {
                            tracing::error!("Dispatcher error: {}", e);
                            dropped += 1;
                        } else {
                            processed += 1;
                        }
                    }
                    _ = shutdown_rx.recv() => {
                        tracing::info!("Dispatcher shutdown requested. Processed: {}, Dropped: {}", processed, dropped);
                        break;
                    }
                }
            }
        });

        // Wait for shutdown signal
        tokio::signal::ctrl_c().await?;
        tracing::info!("Shutdown signal received, initiating graceful shutdown");

        // Trigger shutdown for all components
        let _ = shutdown_tx.send(());

        // Graceful shutdown with timeout
        let shutdown_timeout = Duration::from_secs(self.settings.service.shutdown_timeout_secs);

        let _ = timeout(shutdown_timeout, async {
            let _ = tokio::join!(ads_handle, receiver_handle, dispatcher_handle);
        }).await.or_else(|_| {
            tracing::warn!("Shutdown timeout exceeded, aborting tasks");
            Ok::<_, anyhow::Error>(())
        })?;

        tracing::info!("Log4TC Service gracefully shut down");
        Ok(())
    }
}

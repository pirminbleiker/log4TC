//! AMS/TCP Server for receiving ADS commands from TwinCAT PLCs
//!
//! This server listens on port 48898 and handles AMS/TCP frames containing ADS Write commands.
//! It parses the log data and sends it to the log channel for processing.

use crate::ams::{AmsHeader, AmsNetId, AdsWriteRequest, ADS_CMD_WRITE};
use crate::parser::AdsParser;
use log4tc_core::LogEntry;
use std::net::SocketAddr;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;

/// AMS/TCP Server on port 48898
pub struct AmsTcpServer {
    host: String,
    net_id: AmsNetId,
    port: u16,
    ads_port: u16,
    log_tx: mpsc::Sender<LogEntry>,
}

impl AmsTcpServer {
    /// Create a new AMS/TCP server
    pub fn new(
        host: String,
        net_id: AmsNetId,
        ads_port: u16,
        log_tx: mpsc::Sender<LogEntry>,
    ) -> Self {
        Self {
            host,
            net_id,
            port: 48898,
            ads_port,
            log_tx,
        }
    }

    /// Start the AMS/TCP server
    pub async fn start(&self) -> crate::Result<()> {
        let addr = format!("{}:{}", self.host, self.port);
        let listener = TcpListener::bind(&addr)
            .await
            .map_err(|e| crate::AdsError::BufferError(format!("Failed to bind {}: {}", addr, e)))?;

        tracing::info!("AMS/TCP server listening on {} with Net ID {}", addr, self.net_id.to_string());

        loop {
            let (stream, peer_addr) = listener.accept().await
                .map_err(|e| crate::AdsError::BufferError(format!("Accept error: {}", e)))?;
            tracing::debug!("AMS/TCP connection from {}", peer_addr);

            let net_id = self.net_id;
            let ads_port = self.ads_port;
            let log_tx = self.log_tx.clone();

            tokio::spawn(async move {
                if let Err(e) = Self::handle_connection(stream, peer_addr, net_id, ads_port, log_tx).await {
                    tracing::warn!("AMS/TCP connection error from {}: {}", peer_addr, e);
                }
            });
        }
    }

    async fn handle_connection(
        mut stream: TcpStream,
        peer_addr: SocketAddr,
        _net_id: AmsNetId,
        ads_port: u16,
        log_tx: mpsc::Sender<LogEntry>,
    ) -> crate::Result<()> {
        let mut buf = [0u8; 4096];

        loop {
            // Read TCP/AMS header (6 bytes: reserved + data length)
            let n = stream.read(&mut buf[0..6]).await?;
            if n == 0 {
                tracing::debug!("AMS/TCP connection closed from {}", peer_addr);
                break;
            }

            if n < 6 {
                continue;
            }

            // Parse data length from AMS/TCP header
            let reserved = u16::from_le_bytes([buf[0], buf[1]]);
            let data_len = u32::from_le_bytes([buf[2], buf[3], buf[4], buf[5]]) as usize;

            if reserved != 0 {
                tracing::warn!("Invalid AMS/TCP reserved field: {}", reserved);
                continue;
            }

            if data_len > 4096 - 6 {
                tracing::warn!("AMS/TCP frame too large: {} bytes", data_len);
                continue;
            }

            // Read the remaining AMS header + payload
            stream.read_exact(&mut buf[6..6 + data_len]).await
                .map_err(|e| crate::AdsError::BufferError(format!("Read error: {}", e)))?;

            // Parse and handle the frame
            if let Ok(response) = Self::handle_frame(&buf[6..6 + data_len], ads_port, peer_addr, &log_tx).await {
                // Send response back
                let mut response_buf = vec![0u8; 6 + response.len()];
                response_buf[0..2].copy_from_slice(&0u16.to_le_bytes());
                response_buf[2..6].copy_from_slice(&(response.len() as u32).to_le_bytes());
                response_buf[6..].copy_from_slice(&response);

                if let Err(e) = stream.write_all(&response_buf).await {
                    tracing::warn!("Failed to send AMS response: {}", e);
                    break;
                }
            }
        }

        Ok(())
    }

    async fn handle_frame(
        data: &[u8],
        ads_port: u16,
        peer_addr: SocketAddr,
        log_tx: &mpsc::Sender<LogEntry>,
    ) -> crate::Result<Vec<u8>> {
        // Parse AMS header
        let header = AmsHeader::parse(data)?;

        tracing::debug!("AMS frame received: cmd={}, state={:04x}", header.command_id, header.state_flags);

        // Only process Write commands to ADS port
        if header.command_id != ADS_CMD_WRITE {
            return Err(crate::AdsError::ParseError(
                format!("Unsupported AMS command: {}", header.command_id)
            ));
        }

        if header.target_port != ads_port {
            return Err(crate::AdsError::ParseError(
                format!("Unsupported ADS port: {} (expected {})", header.target_port, ads_port)
            ));
        }

        // Parse ADS Write request
        let payload = &data[32..];
        let write_req = AdsWriteRequest::parse(payload)?;

        // Parse the ADS log entry from write data
        if let Ok(ads_entry) = AdsParser::parse(&write_req.data) {
            // Convert ADS entry to LogEntry
            let source = peer_addr.ip().to_string();
            let hostname = format!("plc-{}", peer_addr.port());

            let mut log_entry = LogEntry::new(
                source,
                hostname,
                ads_entry.message,
                ads_entry.logger,
                ads_entry.level,
            );

            // Copy additional fields from ADS entry
            log_entry.plc_timestamp = ads_entry.plc_timestamp;
            log_entry.clock_timestamp = ads_entry.clock_timestamp;
            log_entry.task_index = ads_entry.task_index;
            log_entry.task_name = ads_entry.task_name;
            log_entry.task_cycle_counter = ads_entry.task_cycle_counter;
            log_entry.app_name = ads_entry.app_name;
            log_entry.project_name = ads_entry.project_name;
            log_entry.online_change_count = ads_entry.online_change_count;
            log_entry.arguments = ads_entry.arguments;
            log_entry.context = ads_entry.context;

            // Send to log channel
            let _ = log_tx.send(log_entry).await;
        }

        // Build response
        let response_header = header.make_response(0);
        let mut response = response_header.serialize();
        response.extend_from_slice(&0u32.to_le_bytes()); // Result: success

        Ok(response)
    }
}

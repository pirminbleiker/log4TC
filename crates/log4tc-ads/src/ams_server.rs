//! AMS/TCP Server for receiving ADS commands from TwinCAT PLCs
//!
//! This server listens on TCP port 48898 (AMS/TCP) and UDP port 48899 (route discovery).
//! It handles AMS/TCP frames containing ADS commands and responds appropriately.

use crate::ams::{
    AdsWriteRequest, AmsHeader, AmsNetId, ADS_CMD_READ, ADS_CMD_READ_DEVICE_INFO,
    ADS_CMD_READ_STATE, ADS_CMD_WRITE,
};
use crate::parser::AdsParser;
use log4tc_core::LogEntry;
use std::net::SocketAddr;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream, UdpSocket};
use tokio::sync::mpsc;

/// AMS/TCP Server on port 48898 + UDP discovery on 48899
pub struct AmsTcpServer {
    host: String,
    net_id: AmsNetId,
    port: u16,
    ads_port: u16,
    log_tx: mpsc::Sender<LogEntry>,
}

impl AmsTcpServer {
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

    pub async fn start(&self) -> crate::Result<()> {
        let tcp_addr = format!("{}:{}", self.host, self.port);
        let udp_addr = format!("{}:{}", self.host, self.port + 1); // 48899

        let tcp_listener = TcpListener::bind(&tcp_addr)
            .await
            .map_err(|e| crate::AdsError::BufferError(format!("Failed to bind TCP {}: {}", tcp_addr, e)))?;

        tracing::info!(
            "AMS/TCP server listening on {} with Net ID {}",
            tcp_addr,
            self.net_id.to_string()
        );

        // Start UDP route discovery listener
        let net_id = self.net_id;
        let udp_host = self.host.clone();
        tokio::spawn(async move {
            if let Err(e) = Self::udp_discovery_listener(&udp_host, net_id).await {
                tracing::warn!("UDP discovery listener error: {}", e);
            }
        });

        // Accept TCP connections
        loop {
            let (stream, peer_addr) = tcp_listener
                .accept()
                .await
                .map_err(|e| crate::AdsError::BufferError(format!("Accept error: {}", e)))?;
            tracing::trace!("AMS/TCP connection from {}", peer_addr);

            let net_id = self.net_id;
            let ads_port = self.ads_port;
            let log_tx = self.log_tx.clone();

            tokio::spawn(async move {
                if let Err(e) =
                    Self::handle_connection(stream, peer_addr, net_id, ads_port, log_tx).await
                {
                    tracing::warn!("AMS/TCP connection error from {}: {}", peer_addr, e);
                }
            });
        }
    }

    /// UDP listener on port 48899 for ADS route discovery
    async fn udp_discovery_listener(host: &str, net_id: AmsNetId) -> crate::Result<()> {
        let addr = format!("{}:48899", host);
        let socket = match UdpSocket::bind(&addr).await {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!("Could not bind UDP discovery on {}: {}", addr, e);
                return Ok(());
            }
        };

        tracing::info!("ADS UDP discovery listening on {}", addr);

        let mut buf = [0u8; 2048];
        loop {
            let (len, src) = match socket.recv_from(&mut buf).await {
                Ok(r) => r,
                Err(e) => {
                    tracing::debug!("UDP recv error: {}", e);
                    continue;
                }
            };

            tracing::debug!("UDP discovery packet from {} ({} bytes)", src, len);

            // Build a minimal route discovery response
            // The response tells the PLC: "I am here, my Net ID is X"
            let response = Self::build_udp_discovery_response(&net_id);
            if let Err(e) = socket.send_to(&response, src).await {
                tracing::debug!("UDP response error: {}", e);
            }
        }
    }

    /// Build UDP route discovery response packet
    fn build_udp_discovery_response(net_id: &AmsNetId) -> Vec<u8> {
        let mut resp = Vec::with_capacity(64);

        // ADS discovery response header
        // Based on Beckhoff ADS protocol: response type + Net ID + name
        resp.extend_from_slice(&[0x03, 0x66, 0x14, 0x71]); // Discovery response magic
        resp.extend_from_slice(&24u32.to_le_bytes()); // Data length

        // Our AMS Net ID (6 bytes)
        let id = net_id.bytes();
        resp.extend_from_slice(id);

        // AMS TCP port (2 bytes)
        resp.extend_from_slice(&48898u16.to_le_bytes());

        // Device name (null-terminated, padded to 16 bytes)
        let mut name = [0u8; 16];
        let src = b"log4tc-rust";
        name[..src.len()].copy_from_slice(src);
        resp.extend_from_slice(&name);

        resp
    }

    async fn handle_connection(
        mut stream: TcpStream,
        peer_addr: SocketAddr,
        _net_id: AmsNetId,
        ads_port: u16,
        log_tx: mpsc::Sender<LogEntry>,
    ) -> crate::Result<()> {
        // Disable Nagle for low-latency responses
        let _ = stream.set_nodelay(true);

        loop {
            // Read AMS/TCP header (6 bytes) using read_exact - handles partial reads correctly
            let mut tcp_header = [0u8; 6];
            match stream.read_exact(&mut tcp_header).await {
                Ok(_) => {}
                Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                    tracing::debug!("AMS/TCP connection closed from {}", peer_addr);
                    break;
                }
                Err(e) => {
                    return Err(crate::AdsError::IoError(e));
                }
            }

            let reserved = u16::from_le_bytes([tcp_header[0], tcp_header[1]]);
            let data_len = u32::from_le_bytes([tcp_header[2], tcp_header[3], tcp_header[4], tcp_header[5]]) as usize;

            if reserved != 0 {
                tracing::warn!("Invalid AMS/TCP reserved field: {}", reserved);
                break; // Protocol error - close connection
            }

            if data_len == 0 || data_len > 1_048_576 {
                tracing::warn!("Invalid AMS/TCP data length: {}", data_len);
                break;
            }

            // Read the AMS header + payload using read_exact
            let mut data = vec![0u8; data_len];
            match stream.read_exact(&mut data).await {
                Ok(_) => {}
                Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                    tracing::debug!("AMS/TCP connection truncated from {}", peer_addr);
                    break;
                }
                Err(e) => {
                    return Err(crate::AdsError::IoError(e));
                }
            }

            // Handle the frame and send response
            match Self::handle_frame(&data, ads_port, peer_addr, &log_tx).await {
                Ok(response) => {
                    // Build AMS/TCP response with header
                    let mut response_buf = Vec::with_capacity(6 + response.len());
                    response_buf.extend_from_slice(&0u16.to_le_bytes()); // Reserved
                    response_buf.extend_from_slice(&(response.len() as u32).to_le_bytes()); // Data length
                    response_buf.extend_from_slice(&response);

                    if let Err(e) = stream.write_all(&response_buf).await {
                        tracing::warn!("Failed to send AMS response to {}: {}", peer_addr, e);
                        break;
                    }
                }
                Err(e) => {
                    tracing::debug!("Frame handling error from {}: {}", peer_addr, e);
                    // Don't break - try to continue with next frame
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
        if data.len() < 32 {
            return Err(crate::AdsError::ParseError("AMS header too short".into()));
        }

        let header = AmsHeader::parse(data)?;

        tracing::trace!(
            "AMS frame: cmd={} src={}:{} -> dst={}:{}",
            header.command_id,
            header.source_net_id.to_string(),
            header.source_port,
            header.target_net_id.to_string(),
            header.target_port,
        );

        match header.command_id {
            ADS_CMD_READ_DEVICE_INFO => {
                tracing::trace!("DeviceInfo request from {}", peer_addr);
                let mut payload = Vec::new();
                payload.extend_from_slice(&0u32.to_le_bytes()); // Result: success
                payload.push(0); // Major version
                payload.push(1); // Minor version
                payload.extend_from_slice(&1u16.to_le_bytes()); // Build number
                let mut name = [0u8; 16];
                let src = b"log4tc-rust";
                name[..src.len()].copy_from_slice(src);
                payload.extend_from_slice(&name);

                let mut rh = header.make_response(0);
                rh.data_length = payload.len() as u32;
                let mut response = rh.serialize();
                response.extend_from_slice(&payload);
                Ok(response)
            }

            ADS_CMD_READ => {
                let payload_data = &data[32..];

                // Parse Read request: IndexGroup(4) + IndexOffset(4) + ReadLength(4)
                if payload_data.len() < 12 {
                    return Err(crate::AdsError::ParseError("Read request too short".into()));
                }

                let index_group = u32::from_le_bytes([payload_data[0], payload_data[1], payload_data[2], payload_data[3]]);
                let index_offset = u32::from_le_bytes([payload_data[4], payload_data[5], payload_data[6], payload_data[7]]);
                let read_length = u32::from_le_bytes([payload_data[8], payload_data[9], payload_data[10], payload_data[11]]);

                tracing::trace!("Read from {} ig={:#x} io={:#x}", peer_addr, index_group, index_offset);

                // Build response: Result(4) + DataLength(4) + Data(N)
                // Return requested amount of zero-filled data with correct DataLength
                let actual_data = vec![0u8; read_length as usize];
                let mut payload = Vec::new();
                payload.extend_from_slice(&0u32.to_le_bytes()); // Result: success
                payload.extend_from_slice(&(actual_data.len() as u32).to_le_bytes()); // DataLength = actual size
                payload.extend_from_slice(&actual_data);

                let mut rh = header.make_response(0);
                rh.data_length = payload.len() as u32;
                let mut response = rh.serialize();
                response.extend_from_slice(&payload);
                Ok(response)
            }

            ADS_CMD_READ_STATE => {
                tracing::trace!("ReadState from {}", peer_addr);
                // Result(4) + AdsState(2) + DeviceState(2)
                let mut payload = Vec::new();
                payload.extend_from_slice(&0u32.to_le_bytes()); // Result: success
                payload.extend_from_slice(&5u16.to_le_bytes()); // ADS State: RUN
                payload.extend_from_slice(&0u16.to_le_bytes()); // Device State: 0

                let mut rh = header.make_response(0);
                rh.data_length = payload.len() as u32;
                let mut response = rh.serialize();
                response.extend_from_slice(&payload);
                Ok(response)
            }

            ADS_CMD_WRITE => {
                let payload = &data[32..];
                let write_req = AdsWriteRequest::parse(payload)?;

                tracing::debug!("ADS Write: {} bytes from {}", write_req.data.len(), peer_addr);

                // Only parse as log entry if targeting our ADS port
                // Buffer can contain MULTIPLE log entries - parse in a loop
                if header.target_port == ads_port {
                    match AdsParser::parse_all(&write_req.data) {
                        Ok(entries) => {
                            for ads_entry in entries {
                                let source = peer_addr.ip().to_string();
                                let hostname = format!("plc-{}", peer_addr.port());

                                let mut log_entry = LogEntry::new(
                                    source,
                                    hostname,
                                    ads_entry.message,
                                    ads_entry.logger,
                                    ads_entry.level,
                                );

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

                                let _ = log_tx.send(log_entry).await;
                            }
                        }
                        Err(e) => {
                            tracing::warn!("Failed to parse log entries: {} (raw {} bytes)", e, write_req.data.len());
                        }
                    }
                }

                // Write response: Result(4) only
                let mut payload = Vec::new();
                payload.extend_from_slice(&0u32.to_le_bytes()); // Result: success

                let mut rh = header.make_response(0);
                rh.data_length = payload.len() as u32;
                let mut response = rh.serialize();
                response.extend_from_slice(&payload);
                Ok(response)
            }

            _ => {
                tracing::debug!("Unknown AMS cmd={} from {} port={}", header.command_id, peer_addr, header.target_port);
                // Respond with success to avoid blocking the PLC
                let mut payload = Vec::new();
                payload.extend_from_slice(&0u32.to_le_bytes()); // Result: success

                let mut rh = header.make_response(0);
                rh.data_length = payload.len() as u32;
                let mut response = rh.serialize();
                response.extend_from_slice(&payload);
                Ok(response)
            }
        }
    }
}

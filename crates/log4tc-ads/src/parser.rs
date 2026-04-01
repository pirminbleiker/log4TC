//! ADS binary protocol parser

use crate::error::*;
use crate::protocol::*;
use chrono::{DateTime, Utc};
use log4tc_core::LogLevel;
use std::collections::HashMap;

// Security limits for protocol parsing
/// Maximum length for individual strings (65 KB)
const MAX_STRING_LENGTH: usize = 65_536;
/// Maximum number of arguments allowed per message
const MAX_ARGUMENTS: usize = 32;
/// Maximum number of context variables allowed per message
const MAX_CONTEXT_VARS: usize = 64;
/// Maximum total message size (1 MB)
const MAX_MESSAGE_SIZE: usize = 1_048_576;

/// Parser for ADS binary protocol messages
pub struct AdsParser;

impl AdsParser {
    /// Parse ALL log entries from a buffer (buffer can contain multiple entries)
    /// Parse ALL log entries from a buffer (PLC sends multiple entries per ADS Write)
    pub fn parse_all(data: &[u8]) -> Result<Vec<AdsLogEntry>> {
        if data.len() > MAX_MESSAGE_SIZE {
            return Err(AdsError::ParseError(
                format!("Message size {} exceeds maximum {}", data.len(), MAX_MESSAGE_SIZE)
            ));
        }

        let mut entries = Vec::new();
        let mut reader = BytesReader::new(data);

        while reader.remaining() > 0 {
            // Skip zero padding
            if reader.peek() == Some(0) {
                break;
            }
            match Self::parse_from_reader(&mut reader) {
                Ok(entry) => entries.push(entry),
                Err(e) => {
                    if entries.is_empty() {
                        return Err(e);
                    }
                    // Partial entry at end - ok, we got what we could
                    tracing::debug!("Partial entry at buffer end ({} bytes remaining): {}", reader.remaining(), e);
                    break;
                }
            }
        }

        Ok(entries)
    }

    /// Parse a single ADS log entry from bytes
    pub fn parse(data: &[u8]) -> Result<AdsLogEntry> {
        if data.len() > MAX_MESSAGE_SIZE {
            return Err(AdsError::ParseError(
                format!("Message size {} exceeds maximum {}", data.len(), MAX_MESSAGE_SIZE)
            ));
        }

        let mut reader = BytesReader::new(data);
        Self::parse_from_reader(&mut reader)
    }

    fn parse_from_reader(reader: &mut BytesReader) -> Result<AdsLogEntry> {
        // Version (1 byte)
        let version_byte = reader.read_u8()?;
        let version =
            AdsProtocolVersion::from_u8(version_byte).ok_or(AdsError::InvalidVersion(version_byte))?;

        // Message (string)
        let message = reader.read_string()?;

        // Logger (string)
        let logger = reader.read_string()?;

        // Level (2 bytes - UINT, PLC uses _WriteUInt for eLogLevel)
        let level_bytes = reader.read_bytes(2)?;
        let level_u16 = u16::from_le_bytes([level_bytes[0], level_bytes[1]]);
        let level = LogLevel::from_u8(level_u16 as u8)
            .ok_or(AdsError::ParseError(format!("Invalid log level: {}", level_u16)))?;

        // Timestamps (8 bytes each, FILETIME format)
        let plc_timestamp = reader.read_filetime()?;
        let clock_timestamp = reader.read_filetime()?;

        // Task metadata
        let task_index = reader.read_i32()?;
        let task_name = reader.read_string()?;
        let task_cycle_counter = reader.read_u32()?;

        // Application metadata
        let app_name = reader.read_string()?;
        let project_name = reader.read_string()?;
        let online_change_count = reader.read_u32()?;

        // Arguments and context
        let mut arguments = HashMap::new();
        let mut context = HashMap::new();

        loop {
            // Check if there's more data
            if reader.remaining() == 0 {
                break;
            }
            let type_id = reader.read_u8()?;
            if type_id == 0 || type_id == 255 {
                // 0 = legacy end marker, 255 = spec end marker
                break;
            }

            if type_id == 1 {
                // Argument - with security limit
                if arguments.len() >= MAX_ARGUMENTS {
                    return Err(AdsError::ParseError(
                        format!("Too many arguments: {} exceeds maximum {}",
                                arguments.len() + 1, MAX_ARGUMENTS)
                    ));
                }
                let index = reader.read_u8()?;
                let value = reader.read_value()?;
                arguments.insert(index as usize, value);
            } else if type_id == 2 {
                // Context - with security limit
                if context.len() >= MAX_CONTEXT_VARS {
                    return Err(AdsError::ParseError(
                        format!("Too many context variables: {} exceeds maximum {}",
                                context.len() + 1, MAX_CONTEXT_VARS)
                    ));
                }
                let scope = reader.read_u8()?;
                let name = reader.read_string()?;
                let value = reader.read_value()?;
                context.insert(format!("scope_{}_{}",scope, name), value);
            }
        }

        Ok(AdsLogEntry {
            version,
            message,
            logger,
            level,
            plc_timestamp,
            clock_timestamp,
            task_index,
            task_name,
            task_cycle_counter,
            app_name,
            project_name,
            online_change_count,
            arguments,
            context,
        })
    }
}

struct BytesReader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> BytesReader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    fn remaining(&self) -> usize {
        self.data.len() - self.pos
    }

    fn peek(&self) -> Option<u8> {
        if self.pos < self.data.len() {
            Some(self.data[self.pos])
        } else {
            None
        }
    }

    fn read_bytes(&mut self, n: usize) -> Result<&'a [u8]> {
        if self.remaining() < n {
            return Err(AdsError::IncompleteMessage {
                expected: n,
                got: self.remaining(),
            });
        }
        let bytes = &self.data[self.pos..self.pos + n];
        self.pos += n;
        Ok(bytes)
    }

    fn read_u8(&mut self) -> Result<u8> {
        Ok(self.read_bytes(1)?[0])
    }

    fn read_i32(&mut self) -> Result<i32> {
        let bytes = self.read_bytes(4)?;
        Ok(i32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    fn read_u32(&mut self) -> Result<u32> {
        let bytes = self.read_bytes(4)?;
        Ok(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    fn read_string(&mut self) -> Result<String> {
        // String format: [Length: u8] + [Data: UTF-8 bytes]
        // PLC FB_LogEntry._WriteString uses _WriteByte(len) - single byte length prefix
        let len = self.read_u8()? as usize;

        // Security: Enforce maximum string length
        if len > MAX_STRING_LENGTH {
            return Err(AdsError::ParseError(
                format!("String length {} exceeds maximum {}", len, MAX_STRING_LENGTH)
            ));
        }

        let str_bytes = self.read_bytes(len)?;

        // Validate UTF-8 first before allocating
        match std::str::from_utf8(str_bytes) {
            Ok(valid_str) => Ok(valid_str.to_string()),
            Err(e) => Err(AdsError::InvalidStringEncoding(e.to_string()))
        }
    }

    fn read_filetime(&mut self) -> Result<DateTime<Utc>> {
        let bytes = self.read_bytes(8)?;
        let filetime = u64::from_le_bytes([
            bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
        ]);

        // FILETIME is 100-nanosecond intervals since 1601-01-01
        // Convert to Unix timestamp (1970-01-01)
        const FILETIME_EPOCH_DIFF: u64 = 116444736000000000; // 100-nanosecond intervals
        if filetime < FILETIME_EPOCH_DIFF {
            return Err(AdsError::InvalidTimestamp(
                "Timestamp before Unix epoch".to_string(),
            ));
        }

        let unix_time_100ns = filetime - FILETIME_EPOCH_DIFF;
        let secs = unix_time_100ns / 10_000_000;
        let nanos = ((unix_time_100ns % 10_000_000) * 100) as u32;

        Ok(DateTime::<Utc>::from_timestamp(secs as i64, nanos)
            .ok_or(AdsError::InvalidTimestamp("Invalid timestamp".to_string()))?)
    }

    fn read_u16(&mut self) -> Result<u16> {
        let bytes = self.read_bytes(2)?;
        Ok(u16::from_le_bytes([bytes[0], bytes[1]]))
    }

    fn read_i16(&mut self) -> Result<i16> {
        let bytes = self.read_bytes(2)?;
        Ok(i16::from_le_bytes([bytes[0], bytes[1]]))
    }

    /// Read a typed value per ADS protocol spec.
    /// Type IDs are INT16 (2 bytes), matching Tc2_Utilities.E_ArgType.
    fn read_value(&mut self) -> Result<serde_json::Value> {
        let val_type = self.read_i16()? as i32;

        match val_type {
            0 => Ok(serde_json::Value::Null),
            1 | 9 => { let v = self.read_u8()?; Ok(serde_json::json!(v)) }         // BYTE/USINT
            2 | 10 => { let v = self.read_u16()?; Ok(serde_json::json!(v)) }       // WORD/UINT
            3 | 11 => { let v = self.read_u32()?; Ok(serde_json::json!(v)) }       // DWORD/UDINT
            4 => {                                                                   // REAL (f32)
                let b = self.read_bytes(4)?;
                Ok(serde_json::json!(f32::from_le_bytes([b[0],b[1],b[2],b[3]])))
            }
            5 => {                                                                   // LREAL (f64)
                let b = self.read_bytes(8)?;
                Ok(serde_json::json!(f64::from_le_bytes([b[0],b[1],b[2],b[3],b[4],b[5],b[6],b[7]])))
            }
            6 => { let v = self.read_u8()? as i8; Ok(serde_json::json!(v)) }       // SINT
            7 => { let v = self.read_i16()?; Ok(serde_json::json!(v)) }             // INT
            8 => { let v = self.read_i32()?; Ok(serde_json::json!(v)) }             // DINT
            12 => { let s = self.read_string()?; Ok(serde_json::Value::String(s)) } // STRING
            13 => { let b = self.read_u8()? != 0; Ok(serde_json::Value::Bool(b)) }  // BOOL
            15 => {                                                                  // ULARGE (u64)
                let b = self.read_bytes(8)?;
                Ok(serde_json::json!(u64::from_le_bytes([b[0],b[1],b[2],b[3],b[4],b[5],b[6],b[7]])))
            }
            17 => {                                                                  // LARGE (i64)
                let b = self.read_bytes(8)?;
                Ok(serde_json::json!(i64::from_le_bytes([b[0],b[1],b[2],b[3],b[4],b[5],b[6],b[7]])))
            }
            20000 => {                                                               // TIME (ms as u32)
                let ms = self.read_u32()?;
                let secs = ms / 1000;
                let millis = ms % 1000;
                Ok(serde_json::Value::String(format!("T#{}s{}ms", secs, millis)))
            }
            20001 => {                                                               // LTIME (100ns as u64)
                let b = self.read_bytes(8)?;
                let ns100 = u64::from_le_bytes([b[0],b[1],b[2],b[3],b[4],b[5],b[6],b[7]]);
                let us = ns100 / 10;
                Ok(serde_json::Value::String(format!("LTIME#{}us", us)))
            }
            20004 => {                                                               // TIME_OF_DAY (ms as u32)
                let ms = self.read_u32()?;
                let h = ms / 3_600_000;
                let m = (ms % 3_600_000) / 60_000;
                let s = (ms % 60_000) / 1000;
                Ok(serde_json::Value::String(format!("TOD#{:02}:{:02}:{:02}", h, m, s)))
            }
            20002 | 20003 => {                                                      // DATE/DT → format as ISO datetime
                let unix_secs = self.read_u32()? as i64;
                let dt = chrono::DateTime::from_timestamp(unix_secs, 0)
                    .unwrap_or_default();
                Ok(serde_json::Value::String(dt.format("%Y-%m-%dT%H:%M:%SZ").to_string()))
            }
            20005 => {                                                               // ENUM (recursive)
                // Read underlying type, then value
                let underlying = self.read_i16()? as i32;
                match underlying {
                    1 | 9 => { let v = self.read_u8()?; Ok(serde_json::json!(v)) }
                    2 | 10 => { let v = self.read_u16()?; Ok(serde_json::json!(v)) }
                    3 | 11 => { let v = self.read_u32()?; Ok(serde_json::json!(v)) }
                    15 => {
                        let b = self.read_bytes(8)?;
                        Ok(serde_json::json!(u64::from_le_bytes([b[0],b[1],b[2],b[3],b[4],b[5],b[6],b[7]])))
                    }
                    _ => {
                        tracing::warn!("Unknown enum underlying type: {}", underlying);
                        Ok(serde_json::Value::Null)
                    }
                }
            }
            20006 => {                                                               // WSTRING (UTF-16LE)
                // Length in characters (1 byte), data is len*2 bytes UTF-16LE
                let char_count = self.read_u8()? as usize;
                let byte_count = char_count * 2;
                let raw = self.read_bytes(byte_count)?;
                // Decode UTF-16LE
                let u16_vals: Vec<u16> = raw.chunks(2)
                    .map(|c| u16::from_le_bytes([c[0], c[1]]))
                    .collect();
                let s = String::from_utf16_lossy(&u16_vals);
                Ok(serde_json::Value::String(s))
            }
            _ => {
                tracing::warn!("Unknown value type: {}", val_type);
                Ok(serde_json::Value::Null)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    // Helper function to build test payloads
    /// Build test payload matching real PLC FB_LogEntry format:
    /// - Strings: 1-byte length prefix (u8) + data
    /// - Level: 2 bytes (u16 LE, _WriteUInt)
    /// - Timestamps: 8 bytes each (FILETIME)
    /// - task_index: 4 bytes (i32, _WriteDInt)
    /// - cycle_counter: 4 bytes (u32, _WriteUDInt)
    /// - online_change_count: 4 bytes (u32, _WriteUDInt)
    fn build_test_payload(message: &str, logger: &str, level: u8) -> Vec<u8> {
        let mut payload = vec![1]; // version byte

        // Message (1-byte len + data)
        payload.push(message.len() as u8);
        payload.extend_from_slice(message.as_bytes());

        // Logger (1-byte len + data)
        payload.push(logger.len() as u8);
        payload.extend_from_slice(logger.as_bytes());

        // Level (2 bytes, u16 LE)
        payload.extend_from_slice(&(level as u16).to_le_bytes());

        // Timestamps (FILETIME: 100-ns intervals since 1601-01-01)
        let unix_now = Utc::now().timestamp() as u64;
        let filetime = (unix_now * 10_000_000) + 116444736000000000;
        payload.extend_from_slice(&filetime.to_le_bytes()); // plc_timestamp
        payload.extend_from_slice(&filetime.to_le_bytes()); // clock_timestamp

        // Task metadata
        payload.extend_from_slice(&1i32.to_le_bytes()); // task_index (_WriteDInt)
        let task_name = "MainTask";
        payload.push(task_name.len() as u8); // 1-byte string len
        payload.extend_from_slice(task_name.as_bytes());
        payload.extend_from_slice(&100u32.to_le_bytes()); // cycle_counter (_WriteUDInt)

        // App metadata
        let app_name = "TestApp";
        payload.push(app_name.len() as u8);
        payload.extend_from_slice(app_name.as_bytes());

        let project_name = "TestProject";
        payload.push(project_name.len() as u8);
        payload.extend_from_slice(project_name.as_bytes());

        payload.extend_from_slice(&0u32.to_le_bytes()); // online_change_count

        // End marker
        payload.push(0);

        payload
    }

    #[test]
    fn test_bytes_reader() {
        let data = vec![1, 2, 3, 4, 5];
        let mut reader = BytesReader::new(&data);
        assert_eq!(reader.read_u8().unwrap(), 1);
        assert_eq!(reader.remaining(), 4);
    }

    #[test]
    fn test_parse_minimal_log_entry() {
        let payload = build_test_payload("Test message", "test.logger", 2);
        let result = AdsParser::parse(&payload);
        assert!(result.is_ok());

        let entry = result.unwrap();
        assert_eq!(entry.message, "Test message");
        assert_eq!(entry.logger, "test.logger");
        assert_eq!(entry.level, LogLevel::Info);
        assert_eq!(entry.version, AdsProtocolVersion::V1);
    }

    #[test]
    fn test_parse_with_all_log_levels() {
        let levels = vec![
            (0, LogLevel::Trace),
            (1, LogLevel::Debug),
            (2, LogLevel::Info),
            (3, LogLevel::Warn),
            (4, LogLevel::Error),
            (5, LogLevel::Fatal),
        ];

        for (level_byte, expected_level) in levels {
            let payload = build_test_payload("Test", "logger", level_byte);
            let entry = AdsParser::parse(&payload).unwrap();
            assert_eq!(entry.level, expected_level, "Level mismatch for byte {}", level_byte);
        }
    }

    #[test]
    fn test_parse_empty_strings() {
        let payload = build_test_payload("", "", 2);
        let result = AdsParser::parse(&payload);
        assert!(result.is_ok());

        let entry = result.unwrap();
        assert_eq!(entry.message, "");
        assert_eq!(entry.logger, "");
    }

    #[test]
    fn test_parse_string_encoding_utf8() {
        let payload = build_test_payload("Hello 世界 🌍", "logger.café", 2);
        let result = AdsParser::parse(&payload);
        assert!(result.is_ok());

        let entry = result.unwrap();
        assert_eq!(entry.message, "Hello 世界 🌍");
        assert_eq!(entry.logger, "logger.café");
    }

    #[test]
    fn test_parse_invalid_version() {
        let mut payload = vec![255]; // Invalid version
        payload.push(1); // message length (1 byte)
        payload.push(b'A');

        let result = AdsParser::parse(&payload);
        assert!(result.is_err());
        match result {
            Err(AdsError::InvalidVersion(v)) => assert_eq!(v, 255),
            _ => panic!("Expected InvalidVersion error"),
        }
    }

    #[test]
    fn test_parse_invalid_log_level() {
        let mut payload = vec![1]; // version
        payload.push(4); // message length (1 byte)
        payload.extend_from_slice(b"test");
        payload.push(6); // logger length (1 byte)
        payload.extend_from_slice(b"logger");
        payload.extend_from_slice(&99u16.to_le_bytes()); // Invalid level (2 bytes)

        let result = AdsParser::parse(&payload);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_incomplete_message() {
        let payload = vec![1, 0, 5]; // Version + incomplete string length
        let result = AdsParser::parse(&payload);
        assert!(result.is_err());
        match result {
            Err(AdsError::IncompleteMessage { .. }) => (),
            _ => panic!("Expected IncompleteMessage error"),
        }
    }

    #[test]
    fn test_parse_buffer_overflow_detection() {
        let mut payload = vec![1]; // version
        payload.push(255); // Claims 255 byte message (1 byte length)
        payload.extend_from_slice(b"short"); // But only provides 5 bytes

        let result = AdsParser::parse(&payload);
        assert!(result.is_err());
    }

    #[test]
    fn test_filetime_conversion() {
        // Test conversion of FILETIME to Unix timestamp
        // FILETIME 132900000000000000 should convert to a valid Unix timestamp
        let payload = build_test_payload("Test", "logger", 2);
        let result = AdsParser::parse(&payload);
        assert!(result.is_ok());

        let entry = result.unwrap();
        // Verify the timestamps are reasonable (within a few seconds of now)
        let now = Utc::now();
        let diff = (now - entry.plc_timestamp).num_seconds().abs();
        assert!(diff < 10, "Parsed timestamp should be close to now, diff: {} seconds", diff);
    }

    #[test]
    fn test_parse_large_message() {
        // Create a message up to 255 bytes (max for 1-byte length prefix)
        let large_message = "x".repeat(255);
        let payload = build_test_payload(&large_message, "logger", 2);
        let result = AdsParser::parse(&payload);
        assert!(result.is_ok());

        let entry = result.unwrap();
        assert_eq!(entry.message.len(), 255);
    }

    #[test]
    fn test_bytes_reader_remaining() {
        let data = vec![1, 2, 3, 4, 5];
        let mut reader = BytesReader::new(&data);
        assert_eq!(reader.remaining(), 5);
        let _ = reader.read_u8();
        assert_eq!(reader.remaining(), 4);
        let _ = reader.read_bytes(2);
        assert_eq!(reader.remaining(), 2);
    }

    #[test]
    fn test_bytes_reader_read_i32() {
        let data: Vec<u8> = 42i32.to_le_bytes().to_vec();
        let mut reader = BytesReader::new(&data);
        assert_eq!(reader.read_i32().unwrap(), 42);
    }

    #[test]
    fn test_bytes_reader_read_u32() {
        let data: Vec<u8> = 1000u32.to_le_bytes().to_vec();
        let mut reader = BytesReader::new(&data);
        assert_eq!(reader.read_u32().unwrap(), 1000);
    }

    #[test]
    fn test_bytes_reader_read_string() {
        let text = "Hello";
        let mut data = vec![text.len() as u8]; // 1-byte length prefix
        data.extend_from_slice(text.as_bytes());

        let mut reader = BytesReader::new(&data);
        assert_eq!(reader.read_string().unwrap(), "Hello");
    }

    #[test]
    fn test_bytes_reader_invalid_utf8() {
        let mut data = vec![2u8]; // 1-byte length prefix
        data.push(0xFF);
        data.push(0xFF); // Invalid UTF-8 sequence

        let mut reader = BytesReader::new(&data);
        let result = reader.read_string();
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_with_positional_arguments() {
        let mut payload = build_test_payload("User {0} logged in", "auth.logger", 2);

        // Add positional argument (value type 8 = DINT)
        // Remove the end marker first
        payload.pop();
        payload.push(1); // type_id = argument
        payload.push(0); // index = 0
        payload.extend_from_slice(&8i16.to_le_bytes()); // value type = DINT (2 bytes)
        payload.extend_from_slice(&123i32.to_le_bytes());
        payload.push(0); // end marker

        let entry = AdsParser::parse(&payload).unwrap();
        assert_eq!(entry.arguments.len(), 1);
        assert_eq!(entry.arguments[&0], serde_json::json!(123));
    }

    #[test]
    fn test_parse_with_context_variables() {
        let mut payload = build_test_payload("Test", "logger", 2);

        // Add context variable
        payload.pop(); // Remove end marker
        payload.push(2); // type_id = context
        payload.push(1); // scope = 1
        let ctx_name = "request_id";
        payload.push(ctx_name.len() as u8); // 1-byte length for string name
        payload.extend_from_slice(ctx_name.as_bytes());
        payload.extend_from_slice(&12i16.to_le_bytes()); // value type = STRING (2 bytes)
        let ctx_value = "req-12345";
        payload.push(ctx_value.len() as u8); // 1-byte length for string value
        payload.extend_from_slice(ctx_value.as_bytes());
        payload.push(0); // end marker

        let entry = AdsParser::parse(&payload).unwrap();
        assert_eq!(entry.context.len(), 1);
        assert_eq!(entry.context["scope_1_request_id"], serde_json::json!("req-12345"));
    }

    #[test]
    fn test_parse_multiple_arguments() {
        let mut payload = build_test_payload("Test {0} {1} {2}", "logger", 2);

        // Add multiple arguments
        payload.pop(); // Remove end marker

        // Argument 0: DINT 42
        payload.push(1); // type_id = argument
        payload.push(0); // index
        payload.extend_from_slice(&8i16.to_le_bytes()); // value type = DINT (2 bytes)
        payload.extend_from_slice(&42i32.to_le_bytes());

        // Argument 1: STRING "test"
        payload.push(1); // type_id = argument
        payload.push(1); // index
        payload.extend_from_slice(&12i16.to_le_bytes()); // value type = STRING (2 bytes)
        payload.push(4); // 1-byte length for "test"
        payload.extend_from_slice(b"test");

        // Argument 2: BOOL true
        payload.push(1); // type_id = argument
        payload.push(2); // index
        payload.extend_from_slice(&13i16.to_le_bytes()); // value type = BOOL (2 bytes)
        payload.push(1); // value = true

        payload.push(0); // end marker

        let entry = AdsParser::parse(&payload).unwrap();
        assert_eq!(entry.arguments.len(), 3);
        assert_eq!(entry.arguments[&0], serde_json::json!(42));
        assert_eq!(entry.arguments[&1], serde_json::json!("test"));
        assert_eq!(entry.arguments[&2], serde_json::json!(true));
    }

    #[test]
    fn test_parse_value_types() {
        let mut payload = build_test_payload("Test", "logger", 2);
        payload.pop(); // Remove end marker

        // Test null
        payload.push(1); // type_id = argument
        payload.push(0); // index
        payload.extend_from_slice(&0i16.to_le_bytes()); // type = null (2 bytes)

        payload.push(0); // end marker

        let entry = AdsParser::parse(&payload).unwrap();
        assert_eq!(entry.arguments[&0], serde_json::Value::Null);
    }

    #[test]
    fn test_parse_float_argument() {
        let mut payload = build_test_payload("Test", "logger", 2);
        payload.pop(); // Remove end marker

        payload.push(1); // type_id = argument
        payload.push(0); // index
        payload.extend_from_slice(&5i16.to_le_bytes()); // value type = LREAL (2 bytes)
        payload.extend_from_slice(&3.14f64.to_le_bytes());

        payload.push(0); // end marker

        let entry = AdsParser::parse(&payload).unwrap();
        let value = &entry.arguments[&0];
        assert!(value.is_number());
    }

    // Additional edge case tests
    #[test]
    fn test_parse_max_u32_value() {
        let mut payload = build_test_payload("Test", "logger", 2);
        payload.pop(); // Remove end marker

        payload.push(1); // type_id = argument
        payload.push(0); // index
        payload.extend_from_slice(&8i16.to_le_bytes()); // value type = DINT (2 bytes)
        payload.extend_from_slice(&(u32::MAX as i32).to_le_bytes());

        payload.push(0); // end marker

        let entry = AdsParser::parse(&payload).unwrap();
        assert!(entry.arguments.contains_key(&0));
    }

    #[test]
    fn test_parse_negative_numbers() {
        let mut payload = build_test_payload("Test", "logger", 2);
        payload.pop(); // Remove end marker

        payload.push(1); // type_id = argument
        payload.push(0); // index
        payload.extend_from_slice(&8i16.to_le_bytes()); // value type = DINT (2 bytes)
        payload.extend_from_slice(&(-42i32).to_le_bytes());

        payload.push(0); // end marker

        let entry = AdsParser::parse(&payload).unwrap();
        assert_eq!(entry.arguments[&0], serde_json::json!(-42));
    }

    #[test]
    fn test_parse_long_context_name() {
        let mut payload = build_test_payload("Test", "logger", 2);
        payload.pop(); // Remove end marker

        payload.push(2); // type_id = context
        payload.push(1); // scope
        let long_name = "x".repeat(1000);
        payload.push(long_name.len() as u8); // 1-byte length for context name
        payload.extend_from_slice(long_name.as_bytes());
        payload.extend_from_slice(&12i16.to_le_bytes()); // value type = STRING (2 bytes)
        let value = "test";
        payload.push(value.len() as u8); // 1-byte length for value
        payload.extend_from_slice(value.as_bytes());

        payload.push(0); // end marker

        let entry = AdsParser::parse(&payload).unwrap();
        assert_eq!(entry.context.len(), 1);
    }

    #[test]
    fn test_parse_many_arguments() {
        let mut payload = build_test_payload("Test", "logger", 2);
        payload.pop(); // Remove end marker

        // Add 32 arguments (the maximum allowed)
        for i in 0..32 {
            payload.push(1); // type_id = argument
            payload.push(i as u8); // index
            payload.extend_from_slice(&8i16.to_le_bytes()); // value type = DINT (2 bytes)
            payload.extend_from_slice(&(i as i32).to_le_bytes());
        }

        payload.push(0); // end marker

        let entry = AdsParser::parse(&payload).unwrap();
        assert_eq!(entry.arguments.len(), 32);
    }

    #[test]
    fn test_parse_roundtrip_utf8_emoji() {
        let emoji_message = "System event 🎉 alert ⚠️ error ❌";
        let payload = build_test_payload(emoji_message, "logger", 2);
        let entry = AdsParser::parse(&payload).unwrap();
        assert_eq!(entry.message, emoji_message);
    }

    #[test]
    fn test_parse_cjk_characters() {
        let cjk_message = "日本語メッセージ 中文信息 한국어 메시지";
        let payload = build_test_payload(cjk_message, "logger", 2);
        let entry = AdsParser::parse(&payload).unwrap();
        assert_eq!(entry.message, cjk_message);
    }

    #[test]
    fn test_parse_control_characters() {
        let msg = "Message\twith\ttabs\nand\nnewlines";
        let payload = build_test_payload(msg, "logger", 2);
        let entry = AdsParser::parse(&payload).unwrap();
        assert_eq!(entry.message, msg);
    }

    #[test]
    fn test_filetime_boundary_unix_epoch() {
        // Test timestamp conversion around Unix epoch boundaries
        let payload = build_test_payload("Test", "logger", 2);
        let entry = AdsParser::parse(&payload).unwrap();

        // Should have valid timestamps
        assert!(entry.plc_timestamp.timestamp() > 0);
        assert!(entry.clock_timestamp.timestamp() > 0);
    }

    #[test]
    fn test_parse_mixed_argument_types() {
        let mut payload = build_test_payload("Test {0} {1} {2} {3} {4}", "logger", 2);
        payload.pop(); // Remove end marker

        // DINT 100
        payload.push(1); // type_id = argument
        payload.push(0); // index
        payload.extend_from_slice(&8i16.to_le_bytes()); // value type = DINT
        payload.extend_from_slice(&100i32.to_le_bytes());

        // LREAL 2.71828
        payload.push(1); // type_id = argument
        payload.push(1); // index
        payload.extend_from_slice(&5i16.to_le_bytes()); // value type = LREAL
        payload.extend_from_slice(&2.71828f64.to_le_bytes());

        // STRING "hello"
        payload.push(1); // type_id = argument
        payload.push(2); // index
        payload.extend_from_slice(&12i16.to_le_bytes()); // value type = STRING
        payload.push(5); // 1-byte length
        payload.extend_from_slice(b"hello");

        // BOOL true
        payload.push(1); // type_id = argument
        payload.push(3); // index
        payload.extend_from_slice(&13i16.to_le_bytes()); // value type = BOOL
        payload.push(1); // true

        // BOOL false
        payload.push(1); // type_id = argument
        payload.push(4); // index
        payload.extend_from_slice(&13i16.to_le_bytes()); // value type = BOOL
        payload.push(0); // false

        payload.push(0); // end marker

        let entry = AdsParser::parse(&payload).unwrap();
        assert_eq!(entry.arguments.len(), 5);
        assert!(entry.arguments[&0].is_number());
        assert!(entry.arguments[&1].is_number());
        assert!(entry.arguments[&2].is_string());
        assert!(entry.arguments[&3].is_boolean());
        assert!(entry.arguments[&4].is_boolean());
    }
}

//! ADS binary protocol parser

use crate::error::*;
use crate::protocol::*;
use bytes::BytesMut;
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
    /// Parse a complete ADS log entry from bytes
    pub fn parse(data: &[u8]) -> Result<AdsLogEntry> {
        // Security: Check total message size
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

        // Level (1 byte)
        let level_byte = reader.read_u8()?;
        let level = LogLevel::from_u8(level_byte)
            .ok_or(AdsError::ParseError("Invalid log level".to_string()))?;

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
            let type_id = reader.read_u8()?;
            if type_id == 0 {
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
        // String format: [Length: u16] + [Data: UTF-8 bytes]
        let len_bytes = self.read_bytes(2)?;
        let len = u16::from_le_bytes([len_bytes[0], len_bytes[1]]) as usize;

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

    fn read_value(&mut self) -> Result<serde_json::Value> {
        // For now, support basic types: 0=null, 1=int, 2=float, 3=string, 4=bool
        let val_type = self.read_u8()?;

        match val_type {
            0 => Ok(serde_json::Value::Null),
            1 => {
                let val = self.read_i32()?;
                Ok(serde_json::json!(val))
            }
            2 => {
                let bytes = self.read_bytes(8)?;
                let val = f64::from_le_bytes([
                    bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6],
                    bytes[7],
                ]);
                Ok(serde_json::json!(val))
            }
            3 => {
                let s = self.read_string()?;
                Ok(serde_json::Value::String(s))
            }
            4 => {
                let b = self.read_u8()? != 0;
                Ok(serde_json::Value::Bool(b))
            }
            _ => Err(AdsError::ParseError(format!("Unknown value type: {}", val_type))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};

    // Helper function to build test payloads
    fn build_test_payload(message: &str, logger: &str, level: u8) -> Vec<u8> {
        let mut payload = vec![1]; // version

        // Message
        let msg_bytes = message.as_bytes();
        payload.extend_from_slice(&(msg_bytes.len() as u16).to_le_bytes());
        payload.extend_from_slice(msg_bytes);

        // Logger
        let logger_bytes = logger.as_bytes();
        payload.extend_from_slice(&(logger_bytes.len() as u16).to_le_bytes());
        payload.extend_from_slice(logger_bytes);

        // Level
        payload.push(level);

        // Timestamps (FILETIME: Unix epoch + 116444736000000000 in 100-ns intervals)
        let unix_now = Utc::now().timestamp() as u64;
        let filetime = (unix_now * 10_000_000) + 116444736000000000;
        payload.extend_from_slice(&filetime.to_le_bytes());
        payload.extend_from_slice(&filetime.to_le_bytes());

        // Task metadata
        payload.extend_from_slice(&1i32.to_le_bytes()); // task_index
        let task_name = "MainTask";
        let task_bytes = task_name.as_bytes();
        payload.extend_from_slice(&(task_bytes.len() as u16).to_le_bytes());
        payload.extend_from_slice(task_bytes);
        payload.extend_from_slice(&100u32.to_le_bytes()); // cycle_counter

        // App metadata
        let app_name = "TestApp";
        let app_bytes = app_name.as_bytes();
        payload.extend_from_slice(&(app_bytes.len() as u16).to_le_bytes());
        payload.extend_from_slice(app_bytes);

        let project_name = "TestProject";
        let proj_bytes = project_name.as_bytes();
        payload.extend_from_slice(&(proj_bytes.len() as u16).to_le_bytes());
        payload.extend_from_slice(proj_bytes);

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
        assert_eq!(entry.level, LogLevel::Information);
        assert_eq!(entry.version, AdsProtocolVersion::V1);
    }

    #[test]
    fn test_parse_with_all_log_levels() {
        let levels = vec![
            (0, LogLevel::Trace),
            (1, LogLevel::Debug),
            (2, LogLevel::Information),
            (3, LogLevel::Warning),
            (4, LogLevel::Error),
            (5, LogLevel::Critical),
            (6, LogLevel::None),
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
        payload.extend_from_slice(&1u16.to_le_bytes()); // message length
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
        payload.extend_from_slice(&4u16.to_le_bytes()); // message length
        payload.extend_from_slice(b"test");
        payload.extend_from_slice(&6u16.to_le_bytes()); // logger length
        payload.extend_from_slice(b"logger");
        payload.push(99); // Invalid level

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
        payload.extend_from_slice(&1000u16.to_le_bytes()); // Claims 1000 byte message
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
        // Create a message larger than typical
        let large_message = "x".repeat(10000);
        let payload = build_test_payload(&large_message, "logger", 2);
        let result = AdsParser::parse(&payload);
        assert!(result.is_ok());

        let entry = result.unwrap();
        assert_eq!(entry.message.len(), 10000);
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
        let mut data = (text.len() as u16).to_le_bytes().to_vec();
        data.extend_from_slice(text.as_bytes());

        let mut reader = BytesReader::new(&data);
        assert_eq!(reader.read_string().unwrap(), "Hello");
    }

    #[test]
    fn test_bytes_reader_invalid_utf8() {
        let mut data = 2u16.to_le_bytes().to_vec();
        data.push(0xFF);
        data.push(0xFF); // Invalid UTF-8 sequence

        let mut reader = BytesReader::new(&data);
        let result = reader.read_string();
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_with_positional_arguments() {
        let mut payload = build_test_payload("User {0} logged in", "auth.logger", 2);

        // Add positional argument (value type 1 = int)
        // Remove the end marker first
        payload.pop();
        payload.push(1); // type_id = argument
        payload.push(0); // index = 0
        payload.push(1); // value type = int
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
        payload.extend_from_slice(&(ctx_name.len() as u16).to_le_bytes());
        payload.extend_from_slice(ctx_name.as_bytes());
        payload.push(3); // value type = string
        let ctx_value = "req-12345";
        payload.extend_from_slice(&(ctx_value.len() as u16).to_le_bytes());
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

        // Argument 0: int 42
        payload.push(1); // type_id = argument
        payload.push(0); // index
        payload.push(1); // value type = int
        payload.extend_from_slice(&42i32.to_le_bytes());

        // Argument 1: string "test"
        payload.push(1); // type_id = argument
        payload.push(1); // index
        payload.push(3); // value type = string
        payload.extend_from_slice(&4u16.to_le_bytes());
        payload.extend_from_slice(b"test");

        // Argument 2: bool true
        payload.push(1); // type_id = argument
        payload.push(2); // index
        payload.push(4); // value type = bool
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
        payload.push(1);
        payload.push(0);
        payload.push(0); // type = null

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
        payload.push(2); // value type = float
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
        payload.push(1); // value type = int
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
        payload.push(1); // value type = int
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
        payload.extend_from_slice(&(long_name.len() as u16).to_le_bytes());
        payload.extend_from_slice(long_name.as_bytes());
        payload.push(3); // value type = string
        let value = "test";
        payload.extend_from_slice(&(value.len() as u16).to_le_bytes());
        payload.extend_from_slice(value.as_bytes());

        payload.push(0); // end marker

        let entry = AdsParser::parse(&payload).unwrap();
        assert_eq!(entry.context.len(), 1);
    }

    #[test]
    fn test_parse_many_arguments() {
        let mut payload = build_test_payload("Test", "logger", 2);
        payload.pop(); // Remove end marker

        // Add 100 arguments
        for i in 0..100 {
            payload.push(1); // type_id = argument
            payload.push(i as u8); // index
            payload.push(1); // value type = int
            payload.extend_from_slice(&(i as i32).to_le_bytes());
        }

        payload.push(0); // end marker

        let entry = AdsParser::parse(&payload).unwrap();
        assert_eq!(entry.arguments.len(), 100);
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

        // int
        payload.push(1);
        payload.push(0);
        payload.push(1);
        payload.extend_from_slice(&100i32.to_le_bytes());

        // float
        payload.push(1);
        payload.push(1);
        payload.push(2);
        payload.extend_from_slice(&2.71828f64.to_le_bytes());

        // string
        payload.push(1);
        payload.push(2);
        payload.push(3);
        payload.extend_from_slice(&5u16.to_le_bytes());
        payload.extend_from_slice(b"hello");

        // bool true
        payload.push(1);
        payload.push(3);
        payload.push(4);
        payload.push(1);

        // bool false
        payload.push(1);
        payload.push(4);
        payload.push(4);
        payload.push(0);

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

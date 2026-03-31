//! Integration tests for ADS parser with real protocol sequences

use log4tc_ads::AdsParser;
use log4tc_core::LogLevel;

fn build_ads_message(message: &str, logger: &str, level: u8) -> Vec<u8> {
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

    // Timestamps
    let filetime = 132900000000000000u64;
    payload.extend_from_slice(&filetime.to_le_bytes());
    payload.extend_from_slice(&filetime.to_le_bytes());

    // Task metadata
    payload.extend_from_slice(&1i32.to_le_bytes());
    let task_name = "Task1";
    let task_bytes = task_name.as_bytes();
    payload.extend_from_slice(&(task_bytes.len() as u16).to_le_bytes());
    payload.extend_from_slice(task_bytes);
    payload.extend_from_slice(&100u32.to_le_bytes());

    // App metadata
    let app_name = "App";
    let app_bytes = app_name.as_bytes();
    payload.extend_from_slice(&(app_bytes.len() as u16).to_le_bytes());
    payload.extend_from_slice(app_bytes);

    let project_name = "Project";
    let proj_bytes = project_name.as_bytes();
    payload.extend_from_slice(&(proj_bytes.len() as u16).to_le_bytes());
    payload.extend_from_slice(proj_bytes);

    payload.extend_from_slice(&0u32.to_le_bytes());

    // End marker
    payload.push(0);

    payload
}

#[test]
fn test_parse_simple_message_sequence() {
    let msg1 = build_ads_message("First message", "logger.a", 2);
    let msg2 = build_ads_message("Second message", "logger.b", 3);

    let entry1 = AdsParser::parse(&msg1).unwrap();
    let entry2 = AdsParser::parse(&msg2).unwrap();

    assert_eq!(entry1.message, "First message");
    assert_eq!(entry1.logger, "logger.a");
    assert_eq!(entry1.level, LogLevel::Info);

    assert_eq!(entry2.message, "Second message");
    assert_eq!(entry2.logger, "logger.b");
    assert_eq!(entry2.level, LogLevel::Warn);
}

#[test]
fn test_parse_all_log_levels_sequence() {
    let levels = vec![
        (0, LogLevel::Trace),
        (1, LogLevel::Debug),
        (2, LogLevel::Info),
        (3, LogLevel::Warn),
        (4, LogLevel::Error),
        (5, LogLevel::Fatal),
    ];

    for (level_byte, expected) in levels {
        let payload = build_ads_message("test", "logger", level_byte);
        let entry = AdsParser::parse(&payload).unwrap();
        assert_eq!(entry.level, expected);
    }
}

#[test]
fn test_parse_realistic_plc_log_message() {
    let realistic_msg =
        "Motor speed reached {0} RPM at cycle {1}";
    let payload = build_ads_message(realistic_msg, "motion.controller", 2);

    let entry = AdsParser::parse(&payload).unwrap();
    assert_eq!(entry.message, realistic_msg);
    assert_eq!(entry.logger, "motion.controller");
}

#[test]
fn test_parse_error_messages_sequence() {
    let error_messages = vec![
        "Configuration error: invalid port number",
        "Runtime error: out of memory",
        "Communication error: timeout exceeded",
        "Critical error: system shutdown",
    ];

    for (i, msg) in error_messages.iter().enumerate() {
        let level = if i < 2 { 4 } else { 5 }; // Error or Fatal
        let payload = build_ads_message(msg, "system.errors", level);
        let entry = AdsParser::parse(&payload).unwrap();
        assert_eq!(entry.message, *msg);
    }
}

#[test]
fn test_parse_unicode_messages_robustness() {
    let messages = vec![
        "Hello 世界",
        "Привет мир",
        "مرحبا بالعالم",
        "שלום עולם",
        "🚀 Emoji test 🎉",
    ];

    for msg in messages {
        let payload = build_ads_message(msg, "i18n.logger", 2);
        let result = AdsParser::parse(&payload);
        assert!(result.is_ok(), "Failed to parse: {}", msg);
        assert_eq!(result.unwrap().message, msg);
    }
}

#[test]
fn test_parse_large_message_handling() {
    let large_msg = "x".repeat(10000);
    let payload = build_ads_message(&large_msg, "logger", 2);
    let entry = AdsParser::parse(&payload).unwrap();
    assert_eq!(entry.message.len(), 10000);
}

#[test]
fn test_parse_special_characters_preservation() {
    let special_msg = r#"Path: C:\Windows\System32, Regex: [a-z]{1,5}, JSON: {"key":"value"}"#;
    let payload = build_ads_message(special_msg, "logger", 2);
    let entry = AdsParser::parse(&payload).unwrap();
    assert_eq!(entry.message, special_msg);
}

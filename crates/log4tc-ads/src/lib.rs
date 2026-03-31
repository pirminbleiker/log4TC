//! ADS (Automation Device Specification) binary protocol parser for Log4TC
//!
//! This crate handles parsing and serialization of the legacy ADS binary protocol
//! used for communication between TwinCAT PLC and the Log4TC service.
//!
//! The ADS protocol is a proprietary Beckhoff protocol for device communication.
//! Log4TC uses ADS for receiving log entries from TwinCAT PLCs.

pub mod protocol;
pub mod parser;
pub mod error;
pub mod listener;

pub use protocol::{AdsLogEntry, AdsProtocolVersion};
pub use parser::AdsParser;
pub use error::{Result, AdsError};
pub use listener::AdsListener;

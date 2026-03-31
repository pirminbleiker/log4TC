# Security Review - Implementer Perspective

## Overview
Conducted comprehensive security review of Log4TC Rust implementation focusing on network exposure, input validation, TLS, and resource limits.

## Findings Summary

### CRITICAL Findings (Task #20)

#### 1. OTEL Exporter - Missing TLS Certificate Validation
**Location**: `crates/log4tc-otel/src/exporter.rs`
**Issue**: HTTP client doesn't enforce TLS 1.2 minimum or certificate validation
**Risk**: Man-in-the-middle attacks possible
**Fix**: Use reqwest::ClientBuilder with TLS configuration

#### 2. ADS Parser - Missing Input Limits
**Location**: `crates/log4tc-ads/src/parser.rs`
**Issue**: No limits on string length, argument count, or message size
**Risk**: DoS via malformed packets, buffer exhaustion
**Limits to add**:
- MAX_STRING_LENGTH: 65,536 bytes
- MAX_ARGUMENTS: 32 items
- MAX_CONTEXT_VARS: 64 items
- MAX_MESSAGE_SIZE: 1 MB

#### 3. Windows Service - Service Account Hardening
**Location**: `crates/log4tc-service/src/windows_service.rs`
**Issue**: Service runs with default account, should use LOCAL SERVICE
**Fix**: Set account_name to `NT AUTHORITY\LOCAL SERVICE`

### HIGH Findings (Task #21)

#### 1. ADS Listener - Missing Connection Limits
**Location**: `crates/log4tc-ads/src/listener.rs`
**Issue**: Unbounded connection spawning allows resource exhaustion
**Fix**: Implement max_connections=100 config limit with enforcement

#### 2. ADS Listener - Missing Connection Timeout
**Location**: `crates/log4tc-ads/src/listener.rs`
**Issue**: Idle connections can block resources indefinitely
**Fix**: Add 300-second read timeout using tokio::time::timeout

#### 3. OTEL Exporter - Hardcoded Auth Headers
**Location**: `crates/log4tc-otel/src/exporter.rs`
**Issue**: Auth headers should not be hardcoded in config
**Fix**: Support environment variable expansion

#### 4. Windows Service - Missing STOP Handler
**Location**: `crates/log4tc-service/src/windows_service.rs`
**Issue**: Service may not stop cleanly
**Fix**: Implement SERVICE_CONTROL_STOP handler

#### 5. Windows Service - Config File ACLs
**Location**: `crates/log4tc-service/src/windows_service.rs`
**Issue**: Config files may have insecure permissions
**Fix**: Set restrictive ACLs during install_service()

## Implementation Plan

1. Implement TLS hardening in OTEL exporter
2. Add input validation limits to ADS parser
3. Harden Windows service account configuration
4. Add connection limits and timeouts to ADS listener
5. Support environment variable expansion for credentials
6. Implement proper Windows service control handlers

## Validation Strategy

- Add unit tests for new input validation
- Test connection limiting under load
- Verify TLS enforcement with mutual TLS tools
- Windows service integration testing

## Timeline

- Task #20: 2-3 hours (CRITICAL fixes)
- Task #21: 2-3 hours (HIGH fixes)
- Verification: 1 hour

Total: ~5 hours for full security hardening

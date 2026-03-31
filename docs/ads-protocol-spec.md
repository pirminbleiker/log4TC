# ADS Binary Protocol Specification - Log4TC v1

**Document Version**: 1.0  
**Protocol Version**: 1  
**Last Updated**: March 31, 2026

---

## Overview

The ADS (Automation Device Specification) binary protocol is a proprietary protocol developed by Beckhoff for communication between TwinCAT automation controllers and external services. Log4TC uses ADS as its legacy protocol for receiving log entries directly from TwinCAT PLCs.

**Key Characteristics**:
- **Type**: Binary, little-endian encoded
- **Transport**: TCP/IP
- **Default Port**: 16150
- **Server Name**: "Log4Tc" (AMS registration)
- **Status**: Maintained for backward compatibility; OTEL is the recommended new protocol

---

## Connection Model

### TCP Server Architecture

```
TwinCAT PLC
    ↓ (TCP connection to port 16150)
    ↓
Log4TC ADS Listener (async TCP server)
    ├─ Binds to: 127.0.0.1:16150 (configurable host:port)
    ├─ Accepts: Multiple concurrent connections
    ├─ Per-connection handler: Async task processing ADS frames
    └─ Response: ACK on success, NAK on failure
```

### Connection Lifecycle

1. **Connect**: TwinCAT initiates TCP connection to listener
2. **Send Message**: TwinCAT sends binary ADS log entry
3. **Parse**: Log4TC parses binary protocol
4. **Respond**: Send ACK (success) or NAK (error)
5. **Disconnect**: Either side can close connection

**Connection Timeout**: 300 seconds (configurable)  
**Max Concurrent Connections**: 100 (configurable via semaphore)

---

## Binary Protocol Format

### Frame Structure (Top-Level)

```
TwinCAT AMS Frame
│
├─ AMS Header (AMS protocol, not detailed here)
│  └─ Source NetId, Source Port
│  └─ Target NetId, Target Port
│  └─ Command ID, State Flags, Data Length
│
└─ AMS Data (Log Entry Payload)
   └─ [Binary Log Entry - see below]
```

### Message Structure (Little-Endian)

```
┌─────────────────────────────────────────────────────────┐
│ Offset │ Length │ Type    │ Field                       │
├─────────────────────────────────────────────────────────┤
│ 0      │ 1      │ uint8   │ Protocol Version (0x01)     │
│ 1      │ *      │ string  │ Message (format template)   │
│ +      │ *      │ string  │ Logger Name                 │
│ +      │ 1      │ uint8   │ Log Level (0-5)             │
│ +      │ 8      │ filetime│ PLC Timestamp               │
│ +      │ 8      │ filetime│ Clock Timestamp (wallclock) │
│ +      │ 4      │ int32   │ Task Index (PID)            │
│ +      │ *      │ string  │ Task Name                   │
│ +      │ 4      │ uint32  │ Task Cycle Counter          │
│ +      │ *      │ string  │ Application Name            │
│ +      │ *      │ string  │ Project Name                │
│ +      │ 4      │ uint32  │ Online Change Count         │
│ +      │ *      │ args[]  │ Arguments (type-value pairs)│
│ +      │ *      │ ctxt[]  │ Context Variables (scope, name, value) │
│ +      │ 1      │ uint8   │ End Marker (0x00)           │
└─────────────────────────────────────────────────────────┘

* = variable length, determined by 2-byte length prefix
+ = offset depends on variable-length fields before it
```

---

## Field Specifications

### 1. Protocol Version (1 byte)

```
Offset: 0
Length: 1 byte
Type:   uint8
Value:  0x01 (version 1)

Current supported versions:
  0x01 = Log4TC v0.2.3+ (current)
  Other values result in ParseError::InvalidVersion
```

### 2. Message (Variable Length String)

```
Format: [Length (2 bytes)] + [Data (UTF-8 bytes)]

Length Encoding:
  - First 2 bytes: uint16 (little-endian) = length in bytes
  - Valid range: 0 to 65,535 bytes
  - Security limit: 65,536 bytes (64 KB)

Example:
  Content: "Motor speed: {0} RPM"
  Encoded: 0x14 0x00 "Motor speed: {0} RPM"
           └─ 0x0014 = 20 bytes
           
Content: Template string with positional placeholders {0}, {1}, etc.
Encoding: UTF-8 without BOM
```

### 3. Logger (Variable Length String)

```
Format: Same as Message field

Content: Logger name identifying source component
Example: "Hardware.Motors.SpeedController"
Limit: 65,536 bytes (security limit)
```

### 4. Log Level (1 byte)

```
Offset: Message_length + Logger_length + 2
Length: 1 byte
Type:   uint8

Mapping:
  0 = Trace   → OTEL SeverityNumber 1 (TRACE)
  1 = Debug   → OTEL SeverityNumber 5 (DEBUG)
  2 = Info    → OTEL SeverityNumber 9 (INFO)
  3 = Warn    → OTEL SeverityNumber 13 (WARN)
  4 = Error   → OTEL SeverityNumber 17 (ERROR)
  5 = Fatal   → OTEL SeverityNumber 21 (FATAL)
  Other values → ParseError

Example:
  0x02 = Info level
```

### 5. PLC Timestamp (8 bytes)

```
Format: FILETIME (Windows 64-bit timestamp)

Encoding:
  - 8 bytes (little-endian uint64)
  - 100-nanosecond intervals since 1601-01-01T00:00:00Z
  - Range: 1601-01-01 to ~2262

Conversion to Unix Epoch:
  1. Read 8 bytes as uint64 (little-endian)
  2. Subtract FILETIME_EPOCH_DIFF = 116,444,736,000,000,000
  3. Divide by 10,000,000 to get seconds
  4. Remainder × 100 = nanoseconds

Example:
  FILETIME: 0x01D5C3A8 0x2B8E3200 (arbitrary)
  → Unix timestamp: [computed from formula above]
  
Security:
  - Timestamps before Unix epoch (1970-01-01) are rejected
  - Nanoseconds validated in range [0, 999,999,999]
```

### 6. Clock Timestamp (8 bytes)

```
Format: FILETIME (same as PLC Timestamp)

Content: Wall-clock time when log entry was recorded
Note: May differ from PLC timestamp due to clock synchronization
Purpose: Establish received time for latency analysis
```

### 7. Task Index (4 bytes)

```
Length: 4 bytes
Type:   int32 (little-endian)
Range:  -2,147,483,648 to 2,147,483,647

Content: Task ID or process ID on the PLC
Example: 1 (main task), 2 (secondary task), etc.

In OTEL: Maps to process.pid attribute
```

### 8. Task Name (Variable Length String)

```
Format: [Length (2 bytes)] + [Data (UTF-8 bytes)]

Content: Human-readable task name
Example: "MainTask", "EventHandler", "DataLogger"
Limit: 65,536 bytes (security limit)

In OTEL: Maps to process.command_line attribute
```

### 9. Task Cycle Counter (4 bytes)

```
Length: 4 bytes
Type:   uint32 (little-endian)
Range:  0 to 4,294,967,295

Content: Cycle number within the task (monotonically increasing)
Purpose: Track execution frequency and detect gaps
Example: 1000, 2000, 3000... (incremented per task cycle)

In OTEL: Maps to task.cycle attribute
```

### 10. Application Name (Variable Length String)

```
Format: [Length (2 bytes)] + [Data (UTF-8 bytes)]

Content: Name of the TwinCAT application
Example: "MotorController", "DataAcquisition"
Limit: 65,536 bytes (security limit)

In OTEL: Maps to service.instance.id attribute
```

### 11. Project Name (Variable Length String)

```
Format: [Length (2 bytes)] + [Data (UTF-8 bytes)]

Content: Name of the TwinCAT project
Example: "AutomationProject2024"
Limit: 65,536 bytes (security limit)

In OTEL: Maps to service.name attribute
```

### 12. Online Change Count (4 bytes)

```
Length: 4 bytes
Type:   uint32 (little-endian)
Range:  0 to 4,294,967,295

Content: Count of online changes (downloads/modifications) to the program
Purpose: Detect program redeployment events
Example: 0 (initial), 1 (after first download), 2 (after second download)

In OTEL: Maps to online.changes attribute
```

### 13. Arguments and Context Section (Variable)

```
Repeated structure until Type = 0:

┌─────────────────────────────────────────┐
│ Type Byte (1 byte)                      │
├─────────────────────────────────────────┤
│  0x00 = End marker (no more args/ctx)   │
│  0x01 = Argument                        │
│  0x02 = Context Variable                │
│  Other = ParseError                     │
└─────────────────────────────────────────┘

────────────────────────────────────────────

ARGUMENTS (Type = 0x01):

  ┌────────────────────────────────────────┐
  │ Type: 0x01 (1 byte)                    │
  │ Index: uint8 (1 byte) - position 0..31 │
  │ Value: Type-tagged value (variable)    │
  └────────────────────────────────────────┘

  Security Limit: 32 arguments maximum per message

────────────────────────────────────────────

CONTEXT VARIABLES (Type = 0x02):

  ┌────────────────────────────────────────┐
  │ Type: 0x02 (1 byte)                    │
  │ Scope: uint8 (1 byte) - context scope  │
  │ Name: String (variable) - variable name│
  │ Value: Type-tagged value (variable)    │
  └────────────────────────────────────────┘

  Security Limit: 64 context variables maximum per message
```

### 14. Type-Tagged Values

Values in arguments and context use a type-tag prefix:

```
┌──────────────────────────────────────────┐
│ Type Byte (1 byte)                       │
├──────────────────────────────────────────┤
│ 0x00 = Null                              │
│ 0x01 = Integer (int32, 4 bytes)          │
│ 0x02 = Float (f64, 8 bytes)              │
│ 0x03 = String (variable length)          │
│ 0x04 = Boolean (1 byte, 0=false/1=true) │
│ Other = ParseError                       │
└──────────────────────────────────────────┘

Examples:

Null:     0x00
Integer:  0x01 0x2A 0x00 0x00 0x00  (value = 42)
Float:    0x02 0x00 0x00 0x00 0x00 0x00 0x00 0xF0 0x3F  (value = 1.0)
String:   0x03 0x05 0x00 "hello"  (5 bytes of "hello")
Boolean:  0x04 0x01  (true)
Boolean:  0x04 0x00  (false)
```

### 15. End Marker (1 byte)

```
Length: 1 byte
Value:  0x00

Purpose: Signals end of arguments and context section
Required: Must be present after all args/context
```

---

## Complete Message Example

### Raw Binary (Hex Dump)

```
01              # Version = 1
06 00           # Message length = 6 bytes
48 69 21 00 00 00  # "Hi!!!" (actually 5 bytes, but let's say "Hi!!!")
05 00           # Actually "Hi!!!" is 5 bytes
48 69 21 21 21  # "Hi!!!"
04 00           # Logger length = 4 bytes
4C 6F 67        # "Log"
02              # Log Level = Info

00 00 00 00     # PLC Timestamp (8 bytes - example value)
00 00 00 00
00 00 00 00

00 00 00 00     # Clock Timestamp (8 bytes - same example)
00 00 00 00

01 00 00 00     # Task Index = 1 (little-endian int32)

09 00           # Task Name length = 9 bytes
4D 61 69 6E 54 61 73 6B  # "MainTask"

64 00 00 00     # Task Cycle Counter = 100 (little-endian uint32)

04 00           # App Name length = 4 bytes
41 70 70        # "App"

07 00           # Project Name length = 7 bytes
50 72 6F 6A 65 63 74  # "Project"

00 00 00 00     # Online Change Count = 0

01              # Argument type
00              # Index = 0
01              # Value type = Integer
2A 00 00 00     # Integer value = 42

00              # End marker - no more args/context
```

---

## Security Constraints

### Parsing Limits

```
┌─────────────────────────┬───────────────┬─────────────────┐
│ Field                   │ Limit         │ Rationale       │
├─────────────────────────┼───────────────┼─────────────────┤
│ String field (any)      │ 65,536 bytes  │ Prevent alloc   │
│ Message payload length  │ 1 MB total    │ DoS protection  │
│ Arguments per message   │ 32 max        │ Prevent abuse   │
│ Context variables       │ 64 max        │ Prevent abuse   │
│ Concurrent connections  │ 100 max       │ Resource limit  │
│ Connection timeout      │ 300 seconds   │ Slowloris mitiga│
└─────────────────────────┴───────────────┴─────────────────┘
```

### Validation Rules

1. **Message Size**: Total payload ≤ 1 MB
2. **String Length**: Each string ≤ 65 KB
3. **UTF-8 Validation**: All strings must be valid UTF-8
4. **Timestamp Validation**: Must be ≥ 1970-01-01 (Unix epoch)
5. **Argument Count**: ≤ 32 per message
6. **Context Count**: ≤ 64 per message
7. **Type Validation**: Type bytes must be 0, 1, 2, 3, or 4
8. **Level Validation**: Log level must be 0-5

---

## Error Responses

### ACK (Success)

```
Response: Single byte
Value:    0x01 (ACK)
Semantics: Message parsed and queued successfully
```

### NAK (Failure)

```
Response: Error code (variable format)
Examples:
  - Protocol version mismatch
  - Incomplete message (not enough bytes)
  - Invalid field values
  - Security limit exceeded (string too long, too many args, etc.)
  - UTF-8 decoding error
  - FILETIME conversion failure

Error Propagation:
  ADS Parser Error → ADS Error Type → NAK to client
```

---

## Performance Characteristics

### Typical Message Sizes

```
Minimal message (no arguments):
  Version (1) + Message (2+5) + Logger (2+3) + Level (1)
  + PLC Time (8) + Clock Time (8) + Task Index (4) + Task Name (2+8)
  + Cycle (4) + App (2+3) + Project (2+3) + Online (4) + End (1)
  = ~60 bytes

Typical message (2 arguments):
  Minimal + 2 args (1 + 1 + 4) = ~75 bytes
  
Maximum message (32 args, 64 context):
  Base + 32 × (1 + 1 + 4) + 64 × (1 + 1 + 2+15 + 4)
  ≈ 1.2 KB (assuming average 15-byte names, simple values)
```

### Throughput

```
At 10,000 msgs/sec with 75 bytes/msg:
  750 KB/sec bandwidth
  10k connections/sec potential load
  ~100ms accumulated latency acceptable
```

---

## Protocol Evolution

### Version 1 (Current)

- 16 distinct data element types
- Type-tagged value system
- FILETIME timestamps
- UTF-8 string encoding
- Argument and context indexing

### Future Versions

Planned enhancements (not yet implemented):
- Compression support (gzip, zstd)
- Structured argument nesting
- Binary encoding for large strings
- Batching multiple entries per frame
- Streaming large values

---

## Reference Implementation

The canonical Rust implementation is in `crates/log4tc-ads/`:

- **parser.rs**: Complete binary protocol parser with security limits
- **listener.rs**: TCP server with connection management
- **protocol.rs**: Type definitions and constants
- **error.rs**: Error types and diagnostics

Key parsing code:
```rust
pub fn parse(data: &[u8]) -> Result<AdsLogEntry> {
    // Total message size check
    if data.len() > MAX_MESSAGE_SIZE { ... }
    
    let mut reader = BytesReader::new(data);
    
    // Parse each field in order with validation
    let version = AdsProtocolVersion::from_u8(reader.read_u8()?)?;
    let message = reader.read_string()?;  // Checks MAX_STRING_LENGTH
    // ... etc ...
    
    // Arguments and context with limits
    loop {
        let type_id = reader.read_u8()?;
        if type_id == 0 { break; }
        
        if arguments.len() >= MAX_ARGUMENTS { ... }
        // or
        if context.len() >= MAX_CONTEXT_VARS { ... }
    }
}
```

---

## Testing

### Test Cases

1. **Valid Messages**: All field types, various sizes
2. **Invalid Versions**: Unsupported version bytes
3. **Incomplete Messages**: Truncated payloads at various offsets
4. **Oversized Strings**: Exceed 65 KB limit
5. **Oversized Message**: Exceed 1 MB total
6. **Invalid UTF-8**: Non-UTF-8 bytes in strings
7. **Invalid Timestamps**: Before Unix epoch
8. **Too Many Arguments**: Exceed 32 limit
9. **Too Many Context Variables**: Exceed 64 limit
10. **Connection Limits**: Test semaphore with 100 max connections
11. **Timeout Handling**: Slow clients (Slowloris-like)

### Integration Tests

- TwinCAT → ADS Listener → Parser → LogEntry conversion
- Concurrent connections and message ordering
- Backpressure on full channel
- Graceful connection closure

---

## Migration Notes

### From ADS to OTEL

**ADS Advantages**:
- Efficient binary protocol
- Low latency
- Compact payloads

**OTEL Advantages**:
- Open standard (not proprietary)
- Multi-language support
- Ecosystem tools and collectors
- Better cloud integration
- More fields and flexibility

**Field Mapping** (see `docs/otel-mapping.md`):
```
ADS Field → OTEL Attribute
project_name → service.name
app_name → service.instance.id
hostname → host.name
logger → logger.name
message → log body
level → severity_number
arguments → attributes
context → attributes
```

---

**Document Status**: Complete  
**Review Date**: March 31, 2026  
**Next Review**: When protocol changes are planned

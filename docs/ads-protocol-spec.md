# ADS Binary Protocol Specification for log4TC

## 1. Overview

This document describes the binary protocol used for transmission of log entries from TwinCAT PLCs to the log4TC receiver service via ADS (Automation Device Specification).

### Purpose
The protocol enables efficient serialization of structured log messages with metadata, context, and typed arguments from PLC programs to a network service for centralized logging and analysis.

### Version
Current protocol version: **1** (single byte header)

### Transport Layer
- **Protocol**: ADS (Automation Device Specification) with WRITE operations
- **Port**: `16150` (hardcoded in service)
- **Index Group**: `1` (hardcoded)
- **Index Offset**: `1` (hardcoded)
- **Connection**: AMS Net ID based communication from PLC to service

---

## 2. ADS Communication Layer

### AMS Net ID
The PLC sends ADS messages with its own AMS Net ID (stored as `T_AmsNetID` string, format: "192.168.1.1.1.1" for 6 bytes).

### Port Details
- ADS port `16150` is registered in the service on startup via `AdsServer(port=16150, name="Log4Tc")`
- The .NET ADS server library (`TwinCAT.Ads.Server`) handles incoming WRITE operations on this port

### Write Operations
ADS WRITE operations deliver log message payloads:
- **WRITE**: Request contains log message buffer(s)
- **SRCADDR**: Pointer to buffer start (PLC side)
- **LEN**: Number of bytes in payload (variable, up to 8KB per task)
- **Response**: `AdsErrorCode.NoError` on success; error code on failure

### Connection Lifecycle

1. **Initialization** (PLC side):
   - Each task creates `FB_Log4TcTask` instance with dual buffers (8KB each)
   - `FB_Log4TcTask.Init(sAmsNetId)` configures the AMS Net ID
   - Service listens on port 16150

2. **Steady State**:
   - Log entries written to active write buffer (no persistent connection needed)
   - When buffer has data, `FB_Log4TcTask.Call()` triggers ADS WRITE
   - Service receives message via `OnWriteAsync()` handler
   - Buffer is swapped for dual-buffering pattern

3. **Error Handling**:
   - ADS errors trigger retry with 5-second wait (TON timer)
   - Service logs errors and continues
   - No graceful shutdown; connection-less design

### Hostname Resolution
After receiving a message, the service calls `AdsHostnameService.GetHostname(target.NetId)` to resolve the sender's hostname from the AMS Net ID.

---

## 3. Binary Message Format

### Overall Structure
Each ADS WRITE payload can contain **one or more log entries** serialized consecutively in a single buffer. The service reads entries sequentially until EOF.

```
Byte Layout (hex):
[Entry1 Version] [Entry1 Data...] [Entry1 Terminator]
[Entry2 Version] [Entry2 Data...] [Entry2 Terminator]
[Entry3 Version] [Entry3 Data...] [Entry3 Terminator]
...
(repeat until EOF)
```

### Message Structure (Pseudo-Code)
```
Payload {
  while position < length {
    LogEntryV1 entry = ReadLogEntry();
  }
}

LogEntryV1 {
  BYTE version = 1;
  String message;
  String logger;
  LogLevel level;       // UINT16
  DateTime plcTimestamp; // FILETIME (LINT)
  DateTime clockTimestamp; // FILETIME (LINT)
  DINT taskIndex;
  String taskName;
  UDINT taskCycleCounter;
  String appName;
  String projectName;
  UDINT onlineChangeCount;
  
  while (true) {
    BYTE type = read();
    switch (type) {
      case 1: // Argument
        BYTE argIndex;
        Object argValue;
        break;
      case 2: // Context
        BYTE scope;
        String ctxName;
        Object ctxValue;
        break;
      case 255: // End
        break out of loop;
    }
  }
}
```

### Payload Size Limits
- **Per-Buffer**: 8192 bytes (`Config.nBufferLen`)
- **Per-Entry**: Variable, depends on message, logger, arguments, and context
- **Timeout**: None (fire-and-forget; retries after 5 seconds on error)

---

## 4. Object Type System

The protocol defines 17 data types that can be serialized in arguments and context values. Each type is identified by a signed 16-bit integer.

### Type ID Reference Table

| Type ID | Type Name | Size (bytes) | C# Type | Encoding Notes |
|---------|-----------|--------------|---------|-----------------|
| 0 | NULL | 0 | null | No data follows |
| 1 | BYTE | 1 | byte | Unsigned 8-bit integer |
| 2 | WORD | 2 | ushort | Unsigned 16-bit integer, little-endian |
| 3 | DWORD | 4 | uint | Unsigned 32-bit integer, little-endian |
| 4 | REAL | 4 | float | IEEE 754 single-precision, little-endian |
| 5 | LREAL | 8 | double | IEEE 754 double-precision, little-endian |
| 6 | SINT | 1 | sbyte | Signed 8-bit integer |
| 7 | INT | 2 | short | Signed 16-bit integer, little-endian |
| 8 | DINT | 4 | int | Signed 32-bit integer, little-endian |
| 9 | USINT | 1 | byte | Unsigned 8-bit integer |
| 10 | UINT | 2 | ushort | Unsigned 16-bit integer, little-endian |
| 11 | UDINT | 4 | uint | Unsigned 32-bit integer, little-endian |
| 12 | STRING | variable | string | Length-prefixed (see String Encoding) |
| 13 | BOOL | 1 | bool | 0x00 = false, 0x01 = true |
| 15 | ULARGE | 8 | ulong | Unsigned 64-bit integer, little-endian |
| 17 | LARGE | 8 | long | Signed 64-bit integer, little-endian |
| 20000 | TIME | 4 | TimeSpan | Milliseconds as UDINT, custom type |
| 20001 | LTIME | 8 | TimeSpan | 100-nanosecond units as ULINT, custom type |
| 20002 | DATE | 4 | DateTimeOffset | Unix seconds as UDINT, custom type |
| 20003 | DATE_AND_TIME | 4 | DateTimeOffset | Unix seconds as UDINT, custom type |
| 20004 | TIME_OF_DAY | 4 | TimeSpan | Milliseconds as UDINT, custom type |
| 20005 | ENUM | 1-8 | object | Recursive: type header + integer value |
| 20006 | WSTRING | variable | string | Wide string (UTF-16), length-prefixed |

---

## 5. Data Type Encoding

### Strings (Type 12, Standard STRING)

**Length-Prefixed, CP1252 Encoded**

```
Byte Layout:
[Length: BYTE] [Data: N bytes]
  1 byte          up to 255 bytes
```

- **Length**: Single unsigned byte (0-255) indicating the number of **bytes** that follow
- **Encoding**: Windows Code Page 1252 (Latin-1)
- **Max Length**: 255 bytes
- **Null Terminator**: Not included in length or serialization; null-terminated in memory on PLC side (`T_MaxString`)

**Example**: String "Log" in CP1252
```
03 4C 6F 67
└─ Length (3 bytes) │ 'L'(0x4C) 'o'(0x6F) 'g'(0x67)
```

### Wide Strings (Type 20006, WSTRING)

**Length-Prefixed, UTF-16LE Encoded**

```
Byte Layout:
[Length: BYTE] [Data: N*2 bytes]
  1 byte          up to 510 bytes (255 chars)
```

- **Length**: Single unsigned byte (0-255) indicating the number of **characters** (not bytes)
- **Encoding**: UTF-16 Little-Endian (Windows wide-char format)
- **Bytes per Char**: 2
- **Max Length**: 255 characters (510 bytes)

**Example**: WSTRING "Hi" in UTF-16LE
```
02 48 00 69 00
└─ Length (2 chars) │ 'H'(0x48,0x00) 'i'(0x69,0x00)
```

### Timestamps (Windows FILETIME)

All datetime values are encoded as **Windows FILETIME** format (64-bit signed integers, little-endian).

**FILETIME Format**:
- **Unit**: 100-nanosecond intervals
- **Epoch**: January 1, 1601 (Windows NT epoch)
- **Conversion**: `DateTime.FromFileTime(int64_value)`

```
Byte Layout:
[Value: LINT (8 bytes, little-endian)]
```

**Example**: January 1, 2000, 00:00:00 UTC
```
- FILETIME value: 0x01BF4D0E4300D400 (125,911,584,000,000,000 in decimal)
- Bytes (little-endian): 00 D4 00 43 0E 4D BF 01
```

On read, if the value is invalid or out of range, fallback to `DateTime(0)` (January 1, 0001).

### Integers

All multi-byte integers use **little-endian** byte order (x86/x64 standard).

| Type | Bytes | Signed | Example |
|------|-------|--------|---------|
| BYTE, USINT | 1 | No | 0xFF |
| SINT | 1 | Yes | 0x80 = -128 |
| WORD, UINT | 2 | No | 0xAB 0xCD → 0xCDAB |
| INT | 2 | Yes | 0xFF 0xFF = -1 |
| DWORD, UDINT | 4 | No | 0x01 0x02 0x03 0x04 → 0x04030201 |
| DINT | 4 | Yes | 0xFF 0xFF 0xFF 0xFF = -1 |
| ULARGE | 8 | No | 8 bytes, little-endian |
| LARGE | 8 | Yes | 8 bytes, little-endian |

### Floats

- **REAL (Type 4)**: IEEE 754 single-precision (4 bytes), little-endian
- **LREAL (Type 5)**: IEEE 754 double-precision (8 bytes), little-endian

### Enumerations (Type 20005)

Enums are serialized as a **recursive object**: the type header is followed by the underlying integer type and value.

```
Byte Layout:
[Type ID: INT16 = 20005] [Underlying Type: INT16] [Value: variable]
```

**Example**: Enum with DWORD value 3
```
E1 4E 03 00 00 00 00
└──────────┘ └──────────────────┘ └───────────────┘
Type 20005     Type 3 (DWORD)     Value 3 (4 bytes)
```

Supported underlying types for enums: BYTE, WORD, DWORD, LWORD (1, 2, 4, or 8 bytes).

---

## 6. LogEntry Structure

The core payload structure is `LogEntryV1`, serialized in the order listed below.

### Field Layout and Serialization Order

| Offset | Field | Type | Size | Encoding | Description |
|--------|-------|------|------|----------|-------------|
| 0 | `version` | BYTE | 1 | Value = 1 | Protocol version identifier |
| 1 | `message` | String | var | CP1252, length-prefixed | Message template (may include placeholders like `{0}`, `{1}`) |
| 1+len_m | `logger` | String | var | CP1252, length-prefixed | Logger name or "_GLOBAL_" |
| ... | `level` | UINT16 | 2 | Little-endian | Log level (0-5, see below) |
| ... | `plcTimestamp` | FILETIME | 8 | Little-endian, signed | PLC system timestamp at log time |
| ... | `clockTimestamp` | FILETIME | 8 | Little-endian, signed | Real-time clock timestamp |
| ... | `taskIndex` | DINT | 4 | Little-endian, signed | Task index from `GETCURTASKINDEXEX()` |
| ... | `taskName` | String | var | CP1252, length-prefixed | Task name from system info |
| ... | `taskCycleCounter` | UDINT | 4 | Little-endian | Cycle count from task info |
| ... | `appName` | String | var | CP1252, length-prefixed | Application name from system info |
| ... | `projectName` | String | var | CP1252, length-prefixed | Project name from system info |
| ... | `onlineChangeCount` | UDINT | 4 | Little-endian | Online change counter |
| ... | `extras` | Variable | var | See section 6.2 | Arguments and context entries |

### Log Level Enumeration

Log levels are serialized as UINT16 values:

| Value | Name | Description |
|-------|------|-------------|
| 0 | Trace | Lowest priority; detailed diagnostic |
| 1 | Debug | Development-level detail |
| 2 | Info | General informational messages |
| 3 | Warn | Warning condition; software continues |
| 4 | Error | Error condition; software impaired |
| 5 | Fatal | Highest priority; severe failure |

### Extras: Arguments and Context (Variable-Length Section)

After the fixed fields, the entry contains zero or more **extra entries** followed by a terminator:

**Type 1: Argument**
```
[Type Byte = 1] [Arg Index: BYTE] [Object: 2+ bytes]
```
- **Type Byte**: 0x01
- **Arg Index**: Positional index for message template replacement (1-based)
- **Object**: Full object encoding (see section 5 for object format)

**Type 2: Context**
```
[Type Byte = 2] [Scope: BYTE] [Context Name: String] [Object: 2+ bytes]
```
- **Type Byte**: 0x02
- **Scope**: E_Scope enum (0=eLog, 1=eLogger, 2=eScoped, 3=eTask)
- **Context Name**: Key name, CP1252 string
- **Object**: Full object encoding

**Terminator**
```
[Type Byte = 255]
```
- **Type Byte**: 0xFF marks the end of an entry

### Full Object Encoding

Objects (arguments and context values) are encoded as:

```
[Type ID: INT16] [Data: variable]
  2 bytes
```

- **Type ID**: Signed 16-bit little-endian integer from section 4
- **Data**: Type-specific serialization (size and format depend on Type ID)

**Example: DWORD Argument**
```
01 01 03 00 00 00 42 00 00 00
│  │  │  └────────────────────┤
│  │  └──── Type ID = 3 (DWORD)
│  └─────── Arg Index = 1
└──────── Type Byte = 1 (Argument)
```

---

## 7. Buffer Management

### Dual-Buffer Pattern (PLC Side)

The `FB_Log4TcTask` function block implements a double-buffering scheme to avoid blocking log writes during transmission.

**Buffer Components** (per task):
- `fbLogBuffer1`: 8192-byte array (from `Config.nBufferLen`)
- `fbLogBuffer2`: 8192-byte array
- `nWriteBuffer`: UINT (1 or 2), indicates which buffer is currently active for writes

**Workflow**:

1. **Write Phase**: Logs are written to the active write buffer
   - `WriteBuffer` property returns pointer to `fbLogBuffer1` or `fbLogBuffer2` based on `nWriteBuffer`
   - Data is appended via `FB_LogEntry.Start()`, `AddAnyArg()`, `AddContext()`, `End()`
   - Writing increments internal byte counter `nBufferCount`

2. **Transmission Phase** (triggered by `FB_Log4TcTask.Call()`):
   ```
   State 10: Wait for log entry; add them to send buffer
     IF WriteBuffer.BufferUsed > 0 THEN
       nWriteBuffer := SEL(nWriteBuffer = 1, 1, 2)  // Toggle buffer
       ADS WRITE with current WriteBuffer data
       → State 20
   
   State 20: Wait for ADS response
     IF ADS done AND NO error THEN
       _ReadBuffer.Clear()  // Clear the buffer that was just sent
       → State 10
     ELSE IF error THEN
       → State 30 (retry)
   
   State 30: Retry wait (5 seconds)
     IF timeout THEN
       Resend same data
       → State 20
   ```

3. **Buffer Swap**: After successful transmission, the non-write buffer (which now contains the sent data) is cleared. The next call to `FB_Log4TcTask.Call()` toggles which buffer is active for new writes.

### Implications for Rust Implementation

- **No guaranteed delivery**: Timeouts and retries may be transient; implement idempotency handling
- **In-order delivery per task**: One task sends one buffer at a time; order is preserved within a task
- **Stateless on service side**: No session, connection pooling, or ack-with-sequence-number protocol
- **Multiple tasks simultaneously**: Multiple PLC tasks can write concurrently; the service receives from all AMS Net IDs on port 16150

---

## 8. Example Payloads

### Example 1: Simple Log Entry (No Arguments)

**Scenario**: Task 1 logs `"System started"` at Info level

**Hex Dump**:
```
01                     // Version = 1
0E 53 79 73 74 65 6D 20 73 74 61 72 74 65 64  // "System started" (14 bytes)
07 5F 47 4C 4F 42 41 4C 5F  // "_GLOBAL_" (9 bytes)
02 00                  // Level = 2 (Info)
00 D4 00 43 0E 4D BF 01  // plcTimestamp (FILETIME for 2000-01-01)
00 D4 00 43 0E 4D BF 01  // clockTimestamp (same)
05 00 00 00            // taskIndex = 5 (DINT)
04 54 41 53 4B         // taskName = "TASK" (4 bytes)
00 00 00 00            // taskCycleCounter = 0
03 50 4C 43            // appName = "PLC" (3 bytes)
07 50 72 6F 6A 65 63 74  // projectName = "Project" (7 bytes)
01 00 00 00            // onlineChangeCount = 1
FF                     // Terminator (end of entry)
```

**Explanation**:
- No arguments (no type 1 entries)
- No context (no type 2 entries)
- Entry is 57 bytes total

### Example 2: Log Entry with One Argument

**Scenario**: Task 2 logs `"Temperature is {0}"` with value 42.5 (REAL) at Warn level

**Hex Dump**:
```
01                          // Version = 1
13 54 65 6D 70 65 72 61 74 75 72 65 20 69 73 20 7B 30 7D  // "Temperature is {0}" (19 bytes)
0C 4C 6F 67 67 65 72 31   // "Logger1" (7 bytes)
03 00                      // Level = 3 (Warn)
00 D4 00 43 0E 4D BF 01   // plcTimestamp
00 D4 00 43 0E 4D BF 01   // clockTimestamp
02 00 00 00               // taskIndex = 2
04 54 41 53 4B           // taskName = "TASK" (4 bytes)
01 00 00 00              // taskCycleCounter = 1
03 50 4C 43             // appName = "PLC"
07 50 72 6F 6A 65 63 74 // projectName = "Project"
02 00 00 00             // onlineChangeCount = 2
01                       // Type = 1 (Argument)
01                       // Arg Index = 1
04 00                    // Object Type = 4 (REAL)
00 00 2A 42             // Value = 42.5 (IEEE 754 float, little-endian)
FF                       // Terminator
```

**Explanation**:
- Single argument with index 1, type REAL, value 42.5
- Entry is ~62 bytes

### Example 3: Log Entry with Context

**Scenario**: Task 3 logs `"Request received"` with context key "request_id" = "REQ001" (STRING)

**Hex Dump**:
```
01                       // Version = 1
10 52 65 71 75 65 73 74 20 72 65 63 65 69 76 65 64  // "Request received" (16 bytes)
07 5F 47 4C 4F 42 41 4C 5F  // "_GLOBAL_"
02 00                    // Level = 2 (Info)
00 D4 00 43 0E 4D BF 01 // plcTimestamp
00 D4 00 43 0E 4D BF 01 // clockTimestamp
03 00 00 00             // taskIndex = 3
04 54 41 53 4B         // taskName = "TASK"
00 00 00 00            // taskCycleCounter = 0
03 50 4C 43           // appName = "PLC"
07 50 72 6F 6A 65 63 74 // projectName = "Project"
00 00 00 00           // onlineChangeCount = 0
02                     // Type = 2 (Context)
00                     // Scope = 0 (eLog)
0A 72 65 71 75 65 73 74 5F 69 64  // "request_id" (10 bytes)
0C 00                  // Object Type = 12 (STRING)
06 52 45 51 30 30 31  // "REQ001" (6 bytes)
FF                     // Terminator
```

**Explanation**:
- One context entry with scope eLog, key "request_id", value "REQ001"
- Entry is ~73 bytes

### Example 4: Multiple Entries in One Payload

A single ADS WRITE can contain multiple entries:

```
[Entry 1: 57 bytes] + [Entry 2: 62 bytes] + [Entry 3: 73 bytes] = 192 bytes total
```

The service reads sequentially with a while loop until `stream.Position >= stream.Length`.

---

## 9. Error Handling

### Buffer Overflow

**PLC Side**:
- `FB_LogEntry._Copy()` checks if `nBufferRemaining >= nCount` before writing
- If insufficient space: sets `bError := TRUE` and logs a warn message via `F_InternalLog()`
- `FB_LogEntry.End()` returns FALSE if error occurred
- Caller should check return value; corrupted entries are not transmitted

**Service Side**:
- No explicit overflow check; assumes PLC side handles it
- If payload is malformed, parsing exception is caught and logged as error
- The entry is discarded; processing continues with next entry if available

### ADS Timeout / Connection Loss

**PLC Side**:
- ADS WRITE operation has built-in timeout (handled by TwinCAT runtime)
- If timeout or error (e.g., service not listening):
  - `fbAdsWriteMsg.ERR` is set to TRUE
  - `fbAdsWriteMsg.ERRID` contains error code
  - State machine enters state 30 (retry) after 5-second wait
  - Same buffer data is retried (no max retries; indefinite)

**Service Side**:
- If service crashes or port closes: PLC will timeout and retry
- When service restarts: new connections are accepted
- No sequence numbers; replayed messages appear as duplicates

### Encoding Errors

**String Encoding**:
- If string > 255 bytes on PLC side: logged as error, entry marked corrupted
- Service registers CP1252 encoding provider on startup; should always decode

**Timestamp Parsing**:
- Service catches `ArgumentException` when FILETIME is out of valid range
- Fallback: uses `DateTime(0)` (January 1, 0001)
- No error logged; processing continues

**Unknown Object Types**:
- Service throws `NotImplementedException($"type {type}")` if type ID not in supported list
- Exception is caught in outer handler; entry logged as error, discarded

### Partial Entry at Buffer Boundary

**Not Possible**: Entries do not span multiple ADS WRITE operations; each ADS message contains complete entries (or none if buffer was not full).

---

## 10. Implementation Notes for Rust

### Parsing Strategy

**Recommended Approach**: Streaming binary parser using a crate like `nom` or `winnow` for robustness.

```rust
use std::io::Read;

fn parse_payload(data: &[u8]) -> Vec<LogEntry> {
    let mut entries = Vec::new();
    let mut cursor = 0;
    
    while cursor < data.len() {
        match parse_log_entry(&data[cursor..]) {
            Ok((consumed, entry)) => {
                entries.push(entry);
                cursor += consumed;
            }
            Err(e) => {
                error!("Parse error at offset {}: {}", cursor, e);
                break;
            }
        }
    }
    
    entries
}

struct LogEntry {
    version: u8,
    message: String,
    logger: String,
    level: LogLevel,
    plc_timestamp: DateTime,
    clock_timestamp: DateTime,
    task_index: i32,
    task_name: String,
    task_cycle_counter: u32,
    app_name: String,
    project_name: String,
    online_change_count: u32,
    arguments: BTreeMap<u8, Object>,
    context: BTreeMap<String, Object>,
}
```

### Zero-Copy Strategies

1. **String Slices**: Avoid copying string data by using byte slices and decoding on-demand
   ```rust
   // Instead of copying entire string:
   let len = data[cursor] as usize;
   cursor += 1;
   let string_bytes = &data[cursor..cursor + len];
   cursor += len;
   // Decode only when needed:
   let string = encoding_rs::WINDOWS_1252.decode(string_bytes).into_owned();
   ```

2. **Borrowed Timestamps**: Parse FILETIME directly without intermediate allocations
   ```rust
   fn parse_filetime(data: &[u8]) -> i64 {
       i64::from_le_bytes([...]) // Direct byte interpretation
   }
   ```

3. **Streaming Parser**: For large payloads, parse and emit entries one at a time without buffering entire payload

### Encoding Support

- **Windows-1252 (CP1252)**: Use `encoding_rs::WINDOWS_1252` crate
- **UTF-16LE**: Standard Rust `String::from_utf16_le()`
- **FILETIME**: Convert to `std::time::SystemTime` or equivalent

```rust
use chrono::DateTime;

fn filetime_to_datetime(ft: i64) -> Option<DateTime<Utc>> {
    const WINDOWS_TICK: i64 = 10_000_000;
    const SEC_TO_UNIX_EPOCH: i64 = 11_644_473_600;
    
    let secs = ft / WINDOWS_TICK - SEC_TO_UNIX_EPOCH;
    DateTime::from_timestamp(secs, 0).ok()
}
```

### Object Type Parsing

Implement a recursive object parser for type-safe deserialization:

```rust
enum Object {
    Null,
    Byte(u8),
    Word(u16),
    DWord(u32),
    Real(f32),
    LReal(f64),
    String(String),
    Bool(bool),
    Time(Duration),
    DateTime(SystemTime),
    Enum(Box<Object>), // Recursive
    // ... other types
}

fn parse_object(data: &[u8], cursor: &mut usize) -> Result<Object, ParseError> {
    let type_id = i16::from_le_bytes([data[*cursor], data[*cursor + 1]]);
    *cursor += 2;
    
    match type_id {
        0 => Ok(Object::Null),
        1 => {
            let val = data[*cursor];
            *cursor += 1;
            Ok(Object::Byte(val))
        }
        // ... handle all 17 types
        20005 => {
            // Enum: parse nested object
            let inner = parse_object(data, cursor)?;
            Ok(Object::Enum(Box::new(inner)))
        }
        _ => Err(ParseError::UnknownType(type_id)),
    }
}
```

### Error Handling

- **Malformed Entries**: Log error, skip entry, continue with next
- **Encoding Errors**: Log warning, use fallback (e.g., `<invalid UTF-8>`)
- **Type Errors**: Use `?` operator or `match` for early exit per-message

### Testing

- **Fuzz Testing**: Generate random payloads to find parser edge cases
- **Known Good Payloads**: Use captured hex dumps from this spec as regression tests
- **Roundtrip**: Encode and decode to verify symmetry (if implementing encoder)

### Performance Considerations

- **Buffer Size**: Expect up to 8KB per message; allocate accordingly
- **Memory Pooling**: Reuse allocations across messages if processing high throughput
- **Async I/O**: Use Tokio or async-std for non-blocking socket handling on port 16150
- **Concurrency**: Each PLC task sends independently; use concurrent map for hostname cache

---

## 11. Appendix: State Machine Diagram

### FB_Log4TcTask Call Sequence

```
┌─────────────┐
│  State 0    │ (Initialization)
│   (Init)    │
└──────┬──────┘
       │
       ▼
┌──────────────────────────────────────┐
│  State 10                            │
│  Wait for log entry in WriteBuffer   │
│  IF WriteBuffer.BufferUsed > 0:      │
│    Toggle nWriteBuffer               │
│    Issue ADS WRITE                   │
│    → State 20                        │
│  ELSE:                               │
│    Loop (wait for next entry)        │
└──────────────────────────────────────┘
       │
       │ (Data sent)
       ▼
┌──────────────────────────────────────┐
│  State 20                            │
│  Wait for ADS response               │
│  IF NOT fbAdsWriteMsg.BUSY:          │
│    IF fbAdsWriteMsg.ERR:             │
│      → State 30 (Retry)              │
│    ELSE:                             │
│      _ReadBuffer.Clear()             │
│      → State 10                      │
│  ELSE:                               │
│    Loop (wait for response)          │
└──────────────────────────────────────┘
       │
       │ (Error)
       ▼
┌──────────────────────────────────────┐
│  State 30                            │
│  Retry wait (5 seconds)              │
│  IF fbRetryWait.Q:                   │
│    fbAdsWriteMsg(WRITE:=TRUE)        │
│    → State 20                        │
│  ELSE:                               │
│    Loop (wait for timeout)           │
└──────────────────────────────────────┘
```

---

## 12. References

- **TwinCAT 3 Documentation**: Automation Device Specification (ADS)
- **Windows FILETIME**: [Microsoft FILETIME structure](https://docs.microsoft.com/en-us/windows/win32/api/minwinbase/ns-minwinbase-filetime)
- **Code Pages**: [Windows-1252 (Latin-1)](https://en.wikipedia.org/wiki/Windows-1252)
- **Rust Encoding**: `encoding_rs` crate for CP1252 support

---

**Document Version**: 1.0  
**Last Updated**: March 2026  
**Protocol Version**: 1

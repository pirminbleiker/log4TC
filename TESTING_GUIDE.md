# Testing Guide for Log4TC Rust Migration

## Overview

This project uses Rust's built-in testing framework with comprehensive unit and integration tests covering all core modules.

## Test Organization

### Unit Tests (In-Module)

Located in each module's `#[cfg(test)]` section:

- **log4tc-core/src/models.rs** - LogLevel, LogEntry, LogRecord tests (25+ tests)
- **log4tc-core/src/formatter.rs** - MessageFormatter tests (30+ tests)
- **log4tc-core/src/config.rs** - Configuration parsing tests (30+ tests)
- **log4tc-ads/src/parser.rs** - ADS protocol parser tests (40+ tests)
- **log4tc-ads/src/protocol.rs** - Protocol constants and types tests (5+ tests)
- **log4tc-otel/src/mapping.rs** - OTEL mapping tests (30+ tests)
- **log4tc-otel/src/receiver.rs** - HTTP/gRPC receiver tests (5+ tests)
- **log4tc-otel/src/exporter.rs** - OTEL exporter tests (30+ tests)

### Integration Tests

Located in `/tests/` directory:

- **tests/integration_parser.rs** - ADS parser protocol sequences (10 tests)
- **tests/integration_mapping.rs** - LogEntry to OTEL flow (10 tests)
- **tests/integration_formatter.rs** - Message formatting scenarios (15 tests)
- **tests/integration_config.rs** - Configuration validation (15 tests)

## Running Tests

### All Tests
```bash
cargo test
```

### Unit Tests Only
```bash
cargo test --lib
```

### Specific Unit Test Module
```bash
# Parser tests
cargo test --lib parser::tests

# Formatter tests
cargo test --lib formatter::tests

# Config tests
cargo test --lib config::tests

# Mapping tests
cargo test --lib mapping::tests
```

### Integration Tests
```bash
# All integration tests
cargo test --test integration_

# Specific integration test
cargo test --test integration_parser
cargo test --test integration_mapping
cargo test --test integration_formatter
cargo test --test integration_config
```

### With Output (Debugging)
```bash
cargo test --lib -- --nocapture
```

### With Backtrace (Error Debugging)
```bash
RUST_BACKTRACE=1 cargo test --lib
```

## Test Coverage

Current coverage by module:

| Module | Tests | Coverage | Key Areas |
|--------|-------|----------|-----------|
| models.rs | 25+ | 90%+ | LogLevel, LogEntry, LogRecord |
| formatter.rs | 30+ | 90%+ | Template formatting, substitution |
| config.rs | 30+ | 85%+ | Configuration parsing, validation |
| parser.rs | 40+ | 95%+ | ADS protocol, edge cases |
| mapping.rs | 30+ | 90%+ | OTEL transformation, attributes |
| protocol.rs | 5+ | 95%+ | Protocol constants |
| receiver.rs | 5+ | 80%+ | HTTP/gRPC setup |
| exporter.rs | 30+ | 85%+ | Payload building, serialization |

**Total Tests**: 195+ unit + 50+ integration = 245+ tests

## Test Categories

### Protocol & Parsing Tests
- Binary message parsing
- FILETIME timestamp conversion
- String encoding (UTF-8, special characters, emoji, CJK)
- All log levels (Trace, Debug, Info, Warn, Error, Fatal)
- Argument types (int, float, bool, string, null)
- Buffer overflow detection
- Invalid protocol handling

### Data Transformation Tests
- LogEntry to OTEL LogRecord mapping
- Field preservation
- Attribute mapping
- Severity number translation
- Resource/scope/log attribute structure

### Formatting Tests
- Positional placeholders {0}, {1}
- Named placeholders {name}
- Mixed placeholders
- Type coercion
- Special character handling
- Placeholder extraction

### Configuration Tests
- TOML/JSON parsing
- Multiple output plugins
- Port and buffer validation
- Serialization roundtrips
- Default values

### Edge Cases
- Empty strings and null values
- Large messages (10KB+, 100KB+)
- Unicode (emoji 🎉, CJK 日本語, Cyrillic, Arabic)
- Many arguments (100+)
- Special characters and escape sequences
- Complex nested objects

## Key Test Scenarios

### Real PLC Message Flow
```rust
// Parser: Raw ADS bytes → AdsLogEntry
// Mapper: AdsLogEntry → LogEntry
// Formatter: Template {0} with args → formatted message
// Config: Receiver/Exporter settings applied
// Result: OTEL-compliant LogRecord
```

### Unicode Support Verification
Tests verify handling of:
- ASCII: `Hello World`
- Emoji: `🚀 System alert 🎉`
- CJK: `日本語メッセージ 中文信息 한국어`
- Cyrillic: `Привет мир`
- Arabic: `مرحبا بالعالم`
- Hebrew: `שלום עולם`

### Error Scenarios
- Invalid protocol version
- Incomplete messages
- Buffer overflow
- Invalid UTF-8 sequences
- Missing/extra arguments
- Invalid timestamps

## Test Data

Helper functions available in each test module:

**Parser tests**: `build_test_payload()` - Creates ADS binary payloads
**Formatter tests**: Example templates and context maps
**Config tests**: Example configuration structures
**Mapping tests**: Example LogEntry instances

## Performance Tests

Performance characteristics tested:
- Parsing: Large messages (>100KB)
- Formatting: 1000+ character templates
- Mapping: Complex nested structures
- Configuration: Multiple output plugins

No strict performance assertions yet - see Task #12 for benchmarking.

## CI/CD Integration

Tests will be integrated with GitHub Actions (Task #15):
- Run on all PRs
- Code coverage reporting
- Minimum coverage threshold (>80%)
- Performance regression detection

## Future Additions

### Property-Based Testing
`proptest` is in dependencies, ready for:
- Roundtrip tests (parse → serialize → parse)
- Invariant checking
- Random input generation

### Async/Network Tests
When integration tests with real sockets added (Task #14):
- ADS listener → parser → handler pipeline
- OTEL exporter → collector communication
- Concurrent client handling

### End-to-End Tests
Planned for Task #14:
- Simulated PLC → service → OTEL collector
- Real docker-compose with testcontainers
- Full pipeline validation

## Troubleshooting

### Test Failures
1. Check error message - usually indicates specific assertion
2. Run with `--nocapture` to see debug output
3. Use `RUST_BACKTRACE=1` for full stack trace
4. Check if related to unicode/encoding issues

### Slow Tests
- Some tests (large message parsing) intentionally slow
- Run with `--test-threads=1` to see individual times
- Not typically bottleneck in normal usage

## Contributing New Tests

When adding new tests:

1. **Unit tests**: Add to module's `#[cfg(test)]` section
2. **Integration tests**: Add to appropriate file in `/tests/`
3. **Naming**: Use `test_<what>_<scenario>` pattern
4. **Documentation**: Comment complex test logic
5. **Coverage**: Test both happy path and error cases
6. **Realism**: Use realistic log messages when possible

## Test Maintenance

- Review test coverage when modifying code
- Update tests when API changes
- Keep test data fixtures simple and focused
- Use helper functions to reduce duplication

---

**Last Updated**: 2024-03-31  
**Test Count**: 245+  
**Target Coverage**: >80%

# Testing Strategy for Log4TC Rust Service

## 1. Overview

### Testing Philosophy

The Log4TC Rust service is a critical bridge between TwinCAT PLCs and observability backends. Our testing strategy prioritizes:

- **Correctness**: Binary protocol parsing and log data transformation must be exact
- **Reliability**: Connection failures, network conditions, and resource exhaustion must be handled gracefully
- **Performance**: Throughput and latency characteristics must meet production requirements
- **Compatibility**: Output must be byte-identical to the legacy .NET service for all 16 object types

### Test Pyramid Approach

```
         ╔════════════════════════╗
         ║   End-to-End Tests     ║  ~10% - Full pipeline with real/simulated PLCs
         ║  (Stress, Acceptance)  ║
         ╠════════════════════════╣
         ║ Integration Tests      ║  ~30% - ADS receiver, OTLP export, config reload
         ║ (testcontainers-rs,    ║
         ║  mock clients)         ║
         ╠════════════════════════╣
         ║     Unit Tests         ║  ~60% - Parsing, mapping, formatting
         ║  (Rust #[test],        ║
         ║  proptest)             ║
         ╚════════════════════════╝
```

## 2. Test Categories

### 2.1 Unit Tests

**Framework**: Rust's built-in `#[test]` framework + `proptest` for property-based testing

**Coverage Areas**:

#### 2.1.1 Binary Protocol Parser
Test the ADS binary protocol parser against known byte sequences.

**Tests**:
- `test_parse_minimal_log_entry` - Minimal valid LogEntry with only required fields
- `test_parse_with_all_fields` - Complete LogEntry with all optional fields
- `test_parse_string_encoding_utf8` - UTF-8 encoded strings with special characters (émojis, CJK)
- `test_parse_string_encoding_code_pages` - Windows CodePages (1252, 932, etc.)
- `test_parse_filetime_conversion` - Windows FILETIME (100-nanosecond intervals) → Unix timestamp
- `test_parse_log_levels_all_six` - All log levels (Trace=0, Debug=1, Info=2, Warn=3, Error=4, Fatal=5)
- `test_parse_arguments_indexed` - Positional arguments (index-based substitution)
- `test_parse_arguments_named` - Named arguments with arbitrary names
- `test_parse_arguments_mixed` - Mixed indexed and named arguments
- `test_parse_context_variables` - Context variables with different scopes
- `test_parse_empty_strings` - Empty message, logger, task name, app name
- `test_parse_large_message` - Message > 64KB (tests length prefix overflow)
- `test_parse_multiple_objects` - Sequential parsing of N objects from single buffer

**Property-Based Tests** (proptest):
- `prop_parse_arbitrary_valid_bytes` - Generate valid protocol bytes, parse, verify no panic
- `prop_parse_roundtrip_symmetric` - Parse → serialize → parse equals original
- `prop_string_length_bounds` - String lengths from 0 to 2^16-1 are handled correctly
- `prop_timestamp_monotonic` - Parsed timestamps preserve ordering of input timestamps

**File location**: `src/parser/tests.rs`

#### 2.1.2 LogEntry → OTEL LogRecord Mapping
Test transformation of parsed LogEntry to OpenTelemetry LogRecord.

**Tests**:
- `test_map_basic_fields` - Message, timestamp, level, logger name map correctly
- `test_map_log_level_translation` - All 6 log levels map to OTEL SeverityNumber (0-24 scale)
- `test_map_timestamp_precision` - Timestamp converted to nanosecond precision without loss
- `test_map_attributes_from_context` - Context variables → OTEL Attributes
- `test_map_trace_context_injection` - TraceId/SpanId from context propagated to LogRecord
- `test_map_span_context_optional` - LogRecord valid without trace context
- `test_map_resource_attributes` - Service name, version, host from config → Resource
- `test_map_scope_attributes` - Instrumentation scope set correctly
- `test_map_empty_message_handling` - Empty message produces valid LogRecord
- `test_map_null_optional_fields` - Missing context/arguments don't cause errors
- `test_map_attribute_limits` - LogRecord respects OTEL attribute count limits (default: no limit, but should handle pruning)
- `test_map_formatted_message_override` - Formatted message preferred over raw message

**File location**: `src/mapping/tests.rs`

#### 2.1.3 Configuration Parsing
Test parsing of YAML/TOML configuration files.

**Tests**:
- `test_parse_minimal_config` - Minimal valid config with only required fields
- `test_parse_full_config` - All configuration options specified
- `test_parse_ads_receiver_config` - Port, AMS name, timeout settings
- `test_parse_otlp_exporter_config` - OTLP endpoint, protocol (gRPC/HTTP), headers
- `test_parse_filter_rules` - Include/exclude logger patterns, level filters
- `test_parse_output_plugins` - Multiple backends (console, file, OTEL)
- `test_parse_invalid_yaml_syntax` - Malformed YAML → descriptive error
- `test_parse_invalid_port_number` - Port out of range → error with suggestion
- `test_parse_environment_variable_substitution` - `${VAR}` and `$VAR` expansion
- `test_parse_default_values` - Missing optional fields use documented defaults
- `test_parse_duplicate_keys` - Duplicate configuration keys → error
- `test_config_hot_reload_schema_validation` - New config validated against schema before apply

**File location**: `src/config/tests.rs`

#### 2.1.4 Message Formatting
Test message template substitution and argument handling.

**Tests**:
- `test_format_no_args` - Message without placeholders returned as-is
- `test_format_positional_args` - `"Foo {0} {1}"` with args → correct substitution
- `test_format_named_args` - `"User: {username}, Role: {role}"` → correct substitution
- `test_format_mixed_args` - Mix of positional and named placeholders
- `test_format_escaped_braces` - `"Cost: {{price}}"` → `"Cost: {price}"`
- `test_format_missing_args` - `"Foo {0}"` with no args → placeholder remains or error (define behavior)
- `test_format_extra_args` - Args provided but not used → formatted message correct
- `test_format_arg_type_coercion` - Integer/boolean/float args formatted as strings
- `test_format_special_chars_in_args` - Newlines, quotes, null bytes in arg values
- `test_format_performance` - Format 1000 messages with 5 args each < 10ms

**File location**: `src/formatting/tests.rs`

#### 2.1.5 Utility Functions
Test helper functions and edge cases.

**Tests**:
- `test_encode_decode_utf8` - String encoding roundtrip
- `test_filetime_to_unix_epoch_boundary` - FILETIME epoch (1601-01-01) converts correctly
- `test_byte_buffer_overflow_detection` - Reading past buffer end detected
- `test_crc_checksum_calculation` - If applicable, verify checksums

**File location**: `src/utils/tests.rs`

**Unit Test Execution**:
```bash
# Run all unit tests
cargo test --lib

# Run specific test module
cargo test --lib parser::tests

# Run with output (failed assertions visible)
cargo test --lib -- --nocapture

# Run with backtrace on failure
RUST_BACKTRACE=1 cargo test --lib
```

---

### 2.2 Integration Tests

**Framework**: Custom test harness + `testcontainers-rs` for OTEL Collector

**Coverage Areas**:

#### 2.2.1 ADS Receiver
Test ADS protocol endpoint receiving and parsing binary payloads.

**Setup**:
```rust
#[tokio::test]
async fn test_ads_receiver_receives_payload() {
    let receiver = AdsReceiver::new(config);
    let client = AdsClient::connect("127.0.0.1:16150").await?;
    
    // Send mock binary payload
    let payload = create_test_log_entry_bytes(...);
    client.write(payload).await?;
    
    // Verify parsed LogEntry appears in channel
    let log_entry = receiver.logs.recv().await?;
    assert_eq!(log_entry.message, expected);
}
```

**Tests**:
- `test_ads_receiver_startup` - Server binds to configured port
- `test_ads_receiver_accepts_connection` - ADS client connects successfully
- `test_ads_receiver_parses_single_payload` - Single binary payload → LogEntry
- `test_ads_receiver_parses_multiple_sequential_payloads` - N payloads in sequence parsed correctly
- `test_ads_receiver_handles_concurrent_clients` - Multiple simultaneous ADS clients
- `test_ads_receiver_rejects_invalid_protocol` - Non-ADS binary → error logged, connection closed
- `test_ads_receiver_timeout_inactive_connection` - Inactive conn closed after timeout
- `test_ads_receiver_logs_connection_events` - Connection/disconnection logged
- `test_ads_receiver_stops_cleanly` - Shutdown signal closes listener and existing conns

**File location**: `tests/integration_ads_receiver.rs`

#### 2.2.2 OTLP Exporter
Test OTLP export endpoint and gRPC/HTTP protocol handling.

**Setup**:
```rust
#[tokio::test]
async fn test_otlp_export_sends_logs() {
    // Start mock OTEL Collector
    let collector = MockOtelCollector::start_on(4317).await?;
    
    let exporter = OtlpExporter::new(OtlpConfig {
        endpoint: "http://127.0.0.1:4317",
        protocol: Protocol::Grpc,
    });
    
    // Export LogRecords
    exporter.export(vec![log_record1, log_record2]).await?;
    
    // Verify collector received them
    let received = collector.received_logs().await?;
    assert_eq!(received.len(), 2);
}
```

**Tests**:
- `test_otlp_export_grpc_success` - gRPC export to collector succeeds
- `test_otlp_export_http_success` - HTTP export to collector succeeds
- `test_otlp_export_batch_aggregation` - Multiple logs batched in single request
- `test_otlp_export_batch_timeout` - Logs exported after timeout even if batch < threshold
- `test_otlp_export_batch_max_size` - Batch respects max payload size
- `test_otlp_export_retry_on_transient_failure` - Transient errors (5xx) retry with backoff
- `test_otlp_export_fails_permanently` - Permanent errors (400, 404) fail after max retries
- `test_otlp_export_collector_unavailable` - All export attempts fail when collector down (tests backpressure)
- `test_otlp_export_with_custom_headers` - Custom headers (auth tokens) sent in requests
- `test_otlp_export_preserves_log_attributes` - All attributes survive export roundtrip

**File location**: `tests/integration_otlp_exporter.rs`

#### 2.2.3 Configuration Hot-Reload
Test runtime configuration updates without restart.

**Setup**:
```rust
#[tokio::test]
async fn test_config_hot_reload() {
    let service = Service::start(config).await?;
    
    // Modify config file on disk
    fs::write("config.yaml", updated_config)?;
    
    // Signal reload
    service.signal_reload().await?;
    
    // Verify new config applied
    assert_eq!(service.current_config().ads_port, NEW_PORT);
}
```

**Tests**:
- `test_reload_updates_ads_port` - Port change affects new connections
- `test_reload_updates_otlp_endpoint` - Endpoint change used for subsequent exports
- `test_reload_updates_filter_rules` - Filter changes affect queued logs
- `test_reload_invalid_config_rejected` - Invalid new config doesn't apply, old config persists
- `test_reload_in_flight_logs_not_affected` - Logs mid-pipeline use original config
- `test_reload_applies_atomically` - Config change visible consistently across threads
- `test_reload_logs_change_summary` - Reload events logged with before/after diff

**File location**: `tests/integration_config_reload.rs`

#### 2.2.4 End-to-End Data Flow
Test complete pipeline: ADS → Parser → Mapper → OTLP.

**Setup**:
```rust
#[tokio::test]
async fn test_e2e_log_flows_through_pipeline() {
    let service = Service::start(test_config()).await?;
    let collector = MockOtelCollector::start().await?;
    
    // Send binary payload via ADS
    let ads_client = AdsClient::connect("127.0.0.1:16150").await?;
    ads_client.write(test_log_bytes()).await?;
    
    // Wait for export
    tokio::time::sleep(Duration::from_millis(100)).await;
    
    // Verify OTEL Collector received matching LogRecord
    let logs = collector.received_logs().await?;
    assert_eq!(logs[0].body.string_value, expected_message);
}
```

**Tests**:
- `test_e2e_single_log_roundtrip` - Single log: ADS in → OTEL out
- `test_e2e_multiple_logs_preserve_order` - N logs maintain ordering through pipeline
- `test_e2e_all_log_levels` - Test all 6 log levels end-to-end
- `test_e2e_attributes_preserved` - Context attributes reach OTEL Collector
- `test_e2e_timestamp_precision_preserved` - Nanosecond precision maintained
- `test_e2e_large_message_handling` - Large messages (>1MB) flow through successfully

**File location**: `tests/integration_e2e.rs`

**Integration Test Execution**:
```bash
# Run all integration tests
cargo test --test integration_*

# Run specific integration test file
cargo test --test integration_ads_receiver

# Run with Docker containers (testcontainers)
TESTCONTAINERS_SKIP_COMPOSE_V2_COMPATIBILITY_CHECK=true cargo test --test integration_otlp_exporter

# Clean up containers after tests
docker container prune -f
```

---

### 2.3 End-to-End Tests

**Framework**: Bash/Python scripts + Docker Compose

**Coverage Areas**:

#### 2.3.1 Full Pipeline with Simulated PLC
Test complete pipeline from simulated TwinCAT PLC to observability backends.

**Setup**:
```bash
#!/bin/bash
# tests/e2e_simulator.sh

# Start OTEL Collector and backends (Jaeger, Loki, etc.)
docker-compose -f tests/docker-compose.e2e.yml up -d

# Start Rust log4TC service
./target/release/log4tc --config tests/e2e-config.yaml &
SERVICE_PID=$!

# Run ADS protocol simulator
python3 tests/ads_simulator.py \
  --logs tests/fixtures/real_logs.bin \
  --target localhost:16150 \
  --interval 100ms

# Verify logs appear in each backend
curl -s http://localhost:16686/api/traces | jq '.data | length' > /tmp/traces.count
curl -s http://localhost:3100/loki/api/v1/query --data-urlencode 'query={job="log4tc"}' | jq '.data.result | length' > /tmp/logs.count

# Assert counts meet expectations
[ $(cat /tmp/traces.count) -ge 100 ] || exit 1
[ $(cat /tmp/logs.count) -ge 100 ] || exit 1

# Cleanup
kill $SERVICE_PID
docker-compose -f tests/docker-compose.e2e.yml down
```

**Tests**:
- `test_e2e_simulator_to_jaeger` - Simulated logs appear in Jaeger UI
- `test_e2e_simulator_to_loki` - Simulated logs appear in Loki/Grafana
- `test_e2e_simulator_to_prometheus` - Metrics (throughput, latency) appear in Prometheus
- `test_e2e_simulator_to_datadog` - Logs reach Datadog (if keys available)
- `test_e2e_simulator_sustained_load` - 1000 msgs/sec for 60s without data loss
- `test_e2e_simulator_various_payload_sizes` - 100B, 1KB, 10KB, 100KB messages

**File location**: `tests/e2e/` directory

#### 2.3.2 Real TwinCAT Runtime (Optional)
Test against actual TwinCAT runtime on a test machine.

**Prerequisites**:
- TwinCAT 3 installation with test project
- log4TC library integrated into test PLC
- Network connectivity to service

**Tests**:
- `test_e2e_real_plc_simple_logs` - PLC sends logs, verify in backends
- `test_e2e_real_plc_task_context` - Task name, cycle counter preserved
- `test_e2e_real_plc_online_change` - Online changes trigger correct context updates
- `test_e2e_real_plc_connection_loss` - PLC disconnects, service handles gracefully

**File location**: `tests/e2e/real_plc.sh`

**E2E Test Execution**:
```bash
# Run simulator-based e2e tests
bash tests/e2e/simulator.sh

# Run with real TwinCAT (requires setup)
bash tests/e2e/real_plc.sh

# View e2e test logs
tail -f tests/e2e.log
```

---

### 2.4 Performance Tests

**Framework**: `criterion.rs` for benchmarking

**Coverage Areas**:

#### 2.4.1 Throughput Benchmarks
Measure messages processed per second at various payload sizes.

```rust
#[criterion::criterion_group(name = "throughput", config = Criterion::default()
    .measurement_time(Duration::from_secs(10))
    .sample_size(100))]

#[criterion::criterion_main(throughput)]

#[criterion::bench_group_config = "throughput"]
fn bench_parse_throughput_small(b: &mut Bencher) {
    // Small payloads (100B)
    let payload = create_test_payload(100);
    b.iter(|| {
        Parser::parse(&payload).unwrap()
    });
}

fn bench_parse_throughput_large(b: &mut Bencher) {
    // Large payloads (100KB)
    let payload = create_test_payload(100_000);
    b.iter(|| {
        Parser::parse(&payload).unwrap()
    });
}

fn bench_otlp_export_throughput(b: &mut Bencher) {
    // 1000 LogRecords per batch
    let records = create_log_records(1000);
    let runtime = tokio::runtime::Runtime::new().unwrap();
    
    b.to_async(&runtime).iter(async {
        exporter.export(records.clone()).await
    });
}
```

**Benchmark targets**:
- `bench_parse_throughput_small` - 100B payloads (target: >10,000 msgs/sec)
- `bench_parse_throughput_medium` - 1KB payloads (target: >5,000 msgs/sec)
- `bench_parse_throughput_large` - 100KB payloads (target: >1,000 msgs/sec)
- `bench_mapping_throughput` - LogEntry → LogRecord conversion (target: >50,000 conversions/sec)
- `bench_otlp_export_throughput_batch_size_100` - Batch 100 (target: <100ms per batch)
- `bench_otlp_export_throughput_batch_size_1000` - Batch 1000 (target: <500ms per batch)
- `bench_format_message_throughput` - Message template substitution (target: >100,000 msgs/sec)

**File location**: `benches/throughput.rs`

#### 2.4.2 Latency Benchmarks
Measure P50, P95, P99 latencies for critical operations.

```rust
fn bench_end_to_end_latency(b: &mut Bencher) {
    // Measure time from ADS receive to OTLP export
    let service = Service::start(config);
    
    b.iter_batched(
        || create_test_payload(1024),
        |payload| async {
            let start = Instant::now();
            service.receive_and_export(payload).await;
            start.elapsed()
        },
        BatchSize::LargeInput,
    );
}
```

**Latency targets**:
- `bench_parse_latency_p50` - Parse 1KB payload: P50 < 1ms
- `bench_parse_latency_p95` - Parse 1KB payload: P95 < 5ms
- `bench_parse_latency_p99` - Parse 1KB payload: P99 < 10ms
- `bench_e2e_latency_p50` - ADS → OTLP: P50 < 50ms
- `bench_e2e_latency_p95` - ADS → OTLP: P95 < 200ms
- `bench_e2e_latency_p99` - ADS → OTLP: P99 < 1000ms

**File location**: `benches/latency.rs`

#### 2.4.3 Memory Usage
Measure memory footprint under sustained load.

```bash
#!/bin/bash
# benches/memory.sh

# Start service and monitor
/usr/bin/time -v ./target/release/log4tc \
    --config benches/memory-config.yaml \
    > /tmp/memory.log 2>&1 &

SERVICE_PID=$!

# Send 10,000 logs over 60 seconds
for i in {1..10000}; do
    echo "payload" | nc localhost 16150
    sleep 0.006  # ~1000 logs/sec
done

# Capture peak memory
PEAK_MEMORY=$(grep "Maximum resident set size" /tmp/memory.log | awk '{print $6}')
echo "Peak memory: $PEAK_MEMORY KB"

# Assert < 500MB at rest
[ $PEAK_MEMORY -lt 500000 ] || exit 1

kill $SERVICE_PID
```

**Memory targets**:
- `mem_idle_baseline` - Service at rest: < 50MB RSS
- `mem_1000_msgs_sec` - Sustained 1k msgs/sec: < 200MB RSS
- `mem_10000_msgs_sec` - Sustained 10k msgs/sec: < 500MB RSS
- `mem_no_leak_over_1h` - Memory stable after 1 hour: no > 10% growth

**File location**: `benches/memory.sh`

#### 2.4.4 CPU Usage
Measure CPU consumption at various throughput levels.

```bash
#!/bin/bash
# benches/cpu.sh

# Monitor CPU with perf
perf record -p $(pgrep log4tc) -F 99 -- sleep 60 &

# Generate load: 5k msgs/sec
python3 benches/load_generator.py --target localhost:16150 --rate 5000

# Analyze
perf report --stdio > /tmp/cpu_profile.txt
```

**CPU targets**:
- `cpu_1000_msgs_sec` - 1k msgs/sec: < 10% CPU (single core)
- `cpu_5000_msgs_sec` - 5k msgs/sec: < 30% CPU
- `cpu_10000_msgs_sec` - 10k msgs/sec: < 60% CPU

**File location**: `benches/cpu.sh`

**Performance Test Execution**:
```bash
# Run all benchmarks
cargo bench

# Run specific benchmark
cargo bench -- throughput

# Generate baseline (commit after stability)
cargo bench -- --save-baseline main

# Compare against baseline
cargo bench -- --baseline main

# Generate flamegraph (requires flamegraph)
cargo flamegraph --bench throughput -- --profile-time 10
```

---

### 2.5 Stress Tests

**Framework**: Custom Rust harness + external load generators

**Coverage Areas**:

#### 2.5.1 Buffer Overflow Scenarios
Test behavior when buffers fill faster than they drain.

```rust
#[tokio::test]
async fn test_stress_buffer_overflow() {
    let config = StressConfig {
        ads_channel_capacity: 10,  // Small buffer
        otlp_export_delay: Duration::from_secs(5),  // Slow export
    };
    
    let service = Service::start(config).await?;
    
    // Blast 1000 messages in rapid succession
    for i in 0..1000 {
        let payload = create_test_payload(1024);
        service.ads_receiver.send(payload)?;
    }
    
    // Verify:
    // - Service doesn't crash (panic)
    // - Messages eventually export (none lost if buffer large enough)
    // - Or messages dropped with backpressure signal (if buffer bounded)
    
    tokio::time::sleep(Duration::from_secs(10)).await;
    assert!(service.is_running());
}
```

**Tests**:
- `test_stress_channel_capacity_exceeded` - Input channel overflows
- `test_stress_unbounded_growth_prevented` - Memory doesn't grow unboundedly
- `test_stress_dropped_messages_logged` - Dropped messages logged as warnings
- `test_stress_backpressure_applied` - ADS client slowed/rejected when overloaded
- `test_stress_recovery_after_drain` - System recovers when load drops

**File location**: `tests/stress_buffer.rs`

#### 2.5.2 Connection Loss and Reconnection
Test graceful handling of network failures.

```rust
#[tokio::test]
async fn test_stress_connection_loss() {
    let service = Service::start(config).await?;
    let client = AdsClient::connect("127.0.0.1:16150").await?;
    
    // Send logs successfully
    client.send_log("msg1").await?;
    assert!(service.log_received("msg1").await);
    
    // Simulate network failure (close connection abruptly)
    drop(client);
    
    // Service should:
    // - Detect disconnection quickly
    // - Log disconnection event
    // - Be ready for new connections
    
    // Reconnect
    let client = AdsClient::connect("127.0.0.1:16150").await?;
    client.send_log("msg2").await?;
    assert!(service.log_received("msg2").await);
}
```

**Tests**:
- `test_stress_client_disconnect_detected` - Disconnection detected within timeout
- `test_stress_multiple_disconnect_reconnect_cycles` - 10 cycles without data loss
- `test_stress_network_timeout_handled` - Idle connection timeout closes gracefully
- `test_stress_partial_payload_received` - Incomplete message doesn't crash parser
- `test_stress_connection_reset_by_peer` - RST TCP packet handled cleanly

**File location**: `tests/stress_connections.rs`

#### 2.5.3 OTEL Collector Unavailable (Backpressure)
Test behavior when export target is down.

```rust
#[tokio::test]
async fn test_stress_otlp_collector_unavailable() {
    let service = Service::start(config).await?;
    
    // Start receiving logs (collector will be unavailable)
    for i in 0..100 {
        let payload = create_test_payload(1024);
        service.ads_receiver.send(payload)?;
    }
    
    // Verify:
    // - Logs queued locally (not lost)
    // - Export retries with backoff (not hammering endpoint)
    // - If buffer bounds reached, new logs either rejected or oldest dropped
    // - No panic or hang
    
    tokio::time::sleep(Duration::from_secs(30)).await;
    
    // Collector comes back online
    let collector = MockOtelCollector::start().await?;
    tokio::time::sleep(Duration::from_secs(5)).await;
    
    // Verify queued logs eventually export
    let exported = collector.received_logs().await?;
    assert!(exported.len() > 50);
}
```

**Tests**:
- `test_stress_collector_down_queues_locally` - Logs buffered when collector unreachable
- `test_stress_collector_down_retries_backoff` - Exponential backoff between retries
- `test_stress_collector_down_timeout` - Retries stop after max attempts/time
- `test_stress_collector_recovery_drains_queue` - When collector recovers, queued logs export
- `test_stress_collector_404_not_retried` - Permanent errors (404) don't retry
- `test_stress_collector_5xx_retried` - Temporary errors (503) retry
- `test_stress_queue_max_size_exceeded` - Queue respects max size (drop or return error)

**File location**: `tests/stress_otlp.rs`

#### 2.5.4 Maximum Concurrent Connections
Test behavior with many simultaneous ADS clients.

```rust
#[tokio::test]
async fn test_stress_concurrent_connections() {
    let service = Service::start(config).await?;
    
    // Connect 100 concurrent ADS clients
    let mut clients = vec![];
    for i in 0..100 {
        let client = AdsClient::connect("127.0.0.1:16150").await?;
        clients.push(client);
    }
    
    // All clients send logs simultaneously
    let handles: Vec<_> = clients
        .into_iter()
        .enumerate()
        .map(|(i, client)| {
            tokio::spawn(async move {
                for j in 0..10 {
                    client.send_log(&format!("msg_{}_{}",  i, j)).await?;
                }
            })
        })
        .collect();
    
    // Wait for all to complete
    for handle in handles {
        handle.await?;
    }
    
    // Verify no data loss
    let expected = 100 * 10;
    let received = service.message_count().await;
    assert_eq!(received, expected);
}
```

**Tests**:
- `test_stress_1_concurrent_connection` - Baseline: 1 client
- `test_stress_10_concurrent_connections` - 10 simultaneous clients
- `test_stress_100_concurrent_connections` - 100 simultaneous clients
- `test_stress_1000_concurrent_connections` - 1000 simultaneous clients (likely to hit OS limits)
- `test_stress_connection_accept_queue_overflow` - Listener backlog exceeded
- `test_stress_long_lived_connections` - Connections stay open 10+ minutes
- `test_stress_rapid_connect_disconnect` - Hammer connect/disconnect cycles

**File location**: `tests/stress_concurrent.rs`

**Stress Test Execution**:
```bash
# Run all stress tests
cargo test --test stress_* -- --test-threads=1 --nocapture

# Run specific stress test
cargo test --test stress_buffer

# Monitor resource usage during stress tests
watch -n 1 'ps aux | grep log4tc'
```

---

### 2.6 Compatibility Tests

**Framework**: Side-by-side comparison of Rust and .NET services

**Coverage Areas**:

#### 2.6.1 Output Comparison with .NET Service
Verify Rust service produces identical output to legacy .NET implementation.

**Setup**:
```rust
#[test]
fn test_compat_output_identical() {
    // Load real ADS payloads captured from production
    let payloads = load_real_payloads("tests/fixtures/real_payloads.bin");
    
    // Parse with both .NET (via subprocess) and Rust
    let rust_results: Vec<_> = payloads
        .iter()
        .map(|p| RustParser::parse(p))
        .collect();
    
    let dotnet_results = invoke_dotnet_parser(&payloads);
    
    // Compare byte-by-byte
    for (i, (rust, dotnet)) in rust_results.iter().zip(dotnet_results.iter()).enumerate() {
        assert_eq!(
            serialize(rust),
            serialize(dotnet),
            "Output mismatch at payload index {}",
            i
        );
    }
}
```

**Tests**:
- `test_compat_all_16_object_types` - All object types parse identically
  - Simple log message
  - With arguments
  - With context variables
  - With trace context
  - etc.
- `test_compat_unicode_handling` - UTF-8 and code pages match exactly
- `test_compat_timestamp_precision` - Timestamps convert identically
- `test_compat_empty_fields_handling` - Empty strings, nulls handled same way
- `test_compat_large_payloads` - >100KB messages parse identically
- `test_compat_malformed_payloads` - Errors occur at same point, same error type

**File location**: `tests/compat_dotnet.rs`

#### 2.6.2 Windows Version Compatibility
Test on various Windows versions without functional differences.

**Test Matrix**:
| Windows Version | Rust Service | OTEL Collector | Status |
|---|---|---|---|
| Windows 10 (21H2) | ✓ | ✓ | Required |
| Windows 11 (22H2) | ✓ | ✓ | Required |
| Windows Server 2019 | ✓ | ✓ | Required |
| Windows Server 2022 | ✓ | ✓ | Required |

**Tests**:
- `test_compat_windows_10_startup` - Service starts on Win10
- `test_compat_windows_11_startup` - Service starts on Win11
- `test_compat_windows_server_2019_startup` - Service starts on Server 2019
- `test_compat_windows_server_2022_startup` - Service starts on Server 2022
- `test_compat_windows_network_stack` - Network stack differences handled (TCP_NODELAY, buffer sizes)
- `test_compat_windows_filetime_handling` - FILETIME conversion correct on all versions

**Execution**:
```bash
# Run on Windows 10 VM
cargo test --test compat_windows -- --test-threads=1

# Run on Windows Server 2022 VM
cargo test --test compat_windows -- --test-threads=1
```

**File location**: `tests/compat_windows.rs`

---

## 3. Test Infrastructure

### 3.1 CI/CD Pipeline (GitHub Actions)

```yaml
# .github/workflows/test.yml

name: Tests

on: [push, pull_request]

jobs:
  unit-tests:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3
      - uses: dtolnay/rust-toolchain@stable
      - name: Run unit tests
        run: cargo test --lib
      - name: Run doc tests
        run: cargo test --doc
      - name: Generate coverage
        uses: taiki-e/tarpaulin-action@v0
        with:
          out: Xml
      - name: Upload coverage
        uses: codecov/codecov-action@v3

  integration-tests:
    runs-on: ubuntu-latest
    services:
      otel-collector:
        image: otel/opentelemetry-collector:latest
        ports:
          - 4317:4317
          - 4318:4318
    steps:
      - uses: actions/checkout@v3
      - uses: dtolnay/rust-toolchain@stable
      - name: Run integration tests
        run: cargo test --test integration_*

  stress-tests:
    runs-on: ubuntu-latest
    if: github.event_name == 'push' && github.ref == 'refs/heads/main'
    steps:
      - uses: actions/checkout@v3
      - uses: dtolnay/rust-toolchain@stable
      - name: Run stress tests (1 hour timeout)
        run: cargo test --test stress_* -- --test-threads=1 --nocapture
        timeout-minutes: 120

  performance-tests:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3
      - uses: dtolnay/rust-toolchain@stable
      - name: Run benchmarks
        run: cargo bench --no-run
      - name: Compare with baseline
        run: cargo bench -- --baseline main || true

  compat-tests:
    runs-on: windows-latest
    steps:
      - uses: actions/checkout@v3
      - uses: dtolnay/rust-toolchain@stable
      - name: Run Windows compatibility tests
        run: cargo test --test compat_windows

  coverage:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3
      - uses: dtolnay/rust-toolchain@stable
      - name: Install coverage tools
        run: cargo install cargo-tarpaulin
      - name: Generate coverage report
        run: cargo tarpaulin --out Xml --exclude-files tests/ benches/
      - name: Upload to codecov
        uses: codecov/codecov-action@v3
```

### 3.2 Test Environments

#### Local Development
```bash
# Prerequisites
rustup update
cargo build

# Run all tests
cargo test

# Run with coverage
cargo tarpaulin --out Html
# Opens coverage/index.html

# Run with output
cargo test -- --nocapture
```

#### CI/CD (GitHub Actions)
- Triggered on: Push to main, all PRs
- Runs on: Ubuntu (unit, integration, stress), Windows (compat)
- Timeout: 120 minutes for stress tests
- Artifacts: Coverage reports, benchmark baselines

#### Staging (Optional)
```bash
# Deploy to staging environment
docker build -t log4tc:staging .
docker-compose -f staging/docker-compose.yml up

# Run acceptance tests against staging
bash tests/e2e/staging.sh

# Performance verification
cargo bench -- --baseline production_baseline
```

### 3.3 Docker Compose for OTEL Collector and Backends

```yaml
# tests/docker-compose.e2e.yml

version: '3.8'

services:
  otel-collector:
    image: otel/opentelemetry-collector:latest
    ports:
      - "4317:4317"    # gRPC
      - "4318:4318"    # HTTP
    volumes:
      - ./otel-config.yaml:/etc/otel-collector-contrib/config.yaml
    command: ["/otelcontribcol", "--config=/etc/otel-collector-contrib/config.yaml"]

  jaeger:
    image: jaegertracing/all-in-one:latest
    ports:
      - "6831:6831/udp"
      - "16686:16686"

  loki:
    image: grafana/loki:latest
    ports:
      - "3100:3100"
    volumes:
      - ./loki-config.yaml:/etc/loki/local-config.yaml
    command: -config.file=/etc/loki/local-config.yaml

  prometheus:
    image: prom/prometheus:latest
    ports:
      - "9090:9090"
    volumes:
      - ./prometheus.yml:/etc/prometheus/prometheus.yml

  grafana:
    image: grafana/grafana:latest
    ports:
      - "3000:3000"
    environment:
      - GF_SECURITY_ADMIN_PASSWORD=admin
```

### 3.4 ADS Protocol Simulator Tool

```rust
// tests/ads_simulator.rs

use clap::Parser;
use std::fs;
use tokio::net::TcpStream;
use tokio::io::AsyncWriteExt;

#[derive(Parser)]
struct Args {
    /// Path to binary log file
    #[arg(short, long)]
    logs: String,

    /// Target address (host:port)
    #[arg(short, long)]
    target: String,

    /// Interval between messages
    #[arg(short, long, default_value = "100ms")]
    interval: String,

    /// Number of messages to send (0 = all)
    #[arg(short, long, default_value = "0")]
    count: usize,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    let logs = fs::read(&args.logs)?;
    let mut stream = TcpStream::connect(&args.target).await?;

    // Parse interval (e.g., "100ms" → Duration)
    let interval = parse_duration(&args.interval)?;

    // Send logs
    let mut offset = 0;
    let mut count = 0;

    while offset < logs.len() && (args.count == 0 || count < args.count) {
        // Each log is prefixed with length (4 bytes)
        let log_len = u32::from_le_bytes([
            logs[offset],
            logs[offset + 1],
            logs[offset + 2],
            logs[offset + 3],
        ]) as usize;

        let log_data = &logs[offset..offset + 4 + log_len];
        stream.write_all(log_data).await?;

        println!("Sent {} bytes", log_data.len());

        offset += 4 + log_len;
        count += 1;

        tokio::time::sleep(interval).await;
    }

    println!("Sent {} logs", count);
    Ok(())
}
```

---

## 4. Test Data

### 4.1 Capturing Real ADS Payloads

**Tools**:
- Wireshark with ADS dissector (packetgraph/ads-dissector)
- Network TAP or SPAN port on switch
- tcpdump filter: `tcp port 16150`

**Procedure**:
```bash
# Capture traffic
sudo tcpdump -i eth0 'tcp port 16150' -w /tmp/ads_traffic.pcap

# Export raw bytes for replay
tshark -r /tmp/ads_traffic.pcap -Y 'ads' -T fields -e ads.data > /tmp/payloads.hex

# Convert hex to binary
xxd -r -p /tmp/payloads.hex > tests/fixtures/real_payloads.bin
```

### 4.2 Generating Synthetic Payloads

```rust
// tests/fixtures/generators.rs

pub fn generate_minimal_entry() -> Vec<u8> {
    let mut buf = Vec::new();
    buf.push(0x01);  // Version
    // Minimal fields...
    buf
}

pub fn generate_entry_with_arguments(arg_count: usize) -> Vec<u8> {
    // Generate N arguments
}

pub fn generate_entry_all_fields() -> Vec<u8> {
    // All optional fields populated
}

pub fn generate_large_message(size_kb: usize) -> Vec<u8> {
    // Message of size_kb kilobytes
}

pub fn generate_invalid_utf8() -> Vec<u8> {
    // String with invalid UTF-8 sequences
}
```

### 4.3 Test Corpus for Fuzzing

```bash
# Use cargo-fuzz to generate corpus
cargo fuzz run parse_log_entry tests/fixtures/

# Or manually place seed files
ls tests/fixtures/parse_corpus/
  - simple_message.bin
  - with_arguments.bin
  - unicode_message.bin
  - malformed_length.bin
  - truncated_payload.bin
```

---

## 5. Coverage Targets

### Unit Tests
- **Target**: >80% code coverage
- **Tool**: `cargo-tarpaulin` or `llvm-cov`
- **Execution**:
  ```bash
  cargo tarpaulin --out Html --exclude-files benches/ tests/
  open tarpaulin-report.html
  ```

### Integration Tests
- **Target**: All critical paths tested (ADS receiver, OTLP exporter, config reload)
- **Measurement**: Number of code paths exercised, not line coverage
- **Execution**:
  ```bash
  cargo test --test integration_* -- --nocapture
  ```

### Coverage Calculation
```bash
# Generate LCOV report
cargo tarpaulin --out Lcov --output-dir coverage

# View in browser
genhtml coverage/lcov.info -o coverage/html
open coverage/html/index.html

# Assert minimum coverage
COVERAGE=$(grep "percent_covered" coverage/lcov.info | awk -F'"' '{print $2}' | cut -d. -f1)
[ $COVERAGE -ge 80 ] || exit 1
```

---

## 6. Regression Testing

### 6.1 Protocol Parser Golden Files

Store expected parse results for known inputs:

```rust
// tests/golden/parser.yaml

- name: "simple message"
  input: "tests/fixtures/simple_message.bin"
  expected:
    message: "Hello, World!"
    logger: "MyApp"
    level: 2  # Info
    timestamp: 1234567890000000000

- name: "with arguments"
  input: "tests/fixtures/with_arguments.bin"
  expected:
    message: "User {0} logged in"
    formatted_message: "User alice logged in"
    arguments:
      - "alice"
```

**Test**:
```rust
#[test]
fn test_golden_files_regression() {
    let golden = load_golden_file("tests/golden/parser.yaml");
    
    for test_case in golden {
        let input = fs::read(&test_case.input)?;
        let parsed = Parser::parse(&input)?;
        
        assert_eq!(parsed.message, test_case.expected.message);
        assert_eq!(parsed.logger, test_case.expected.logger);
        // ... more assertions
    }
}
```

### 6.2 OTEL Mapping Snapshot Tests

Use `insta` crate for snapshot testing:

```rust
#[test]
fn test_otel_mapping_snapshots() {
    let log_entry = create_test_log_entry();
    let otel_record = LogentryToOtelMapper::map(&log_entry);
    
    insta::assert_json_snapshot!(otel_record);
}
```

Run with `cargo insta test` and review snapshots with `cargo insta review`.

### 6.3 Performance Regression Baselines

```bash
# Establish baseline on main branch
cargo bench -- --save-baseline main

# After changes, compare
cargo bench -- --baseline main

# Fail CI if >10% regression
cargo bench -- --baseline main | grep -i "regression" && exit 1
```

---

## 7. Acceptance Criteria

All of the following must pass before production deployment:

### Functionality
- [ ] All 16 ADS object types parse identically to .NET service
- [ ] All LogEntry fields map correctly to OTEL LogRecord
- [ ] Configuration hot-reload works without downtime
- [ ] All 6 log levels (Trace, Debug, Info, Warn, Error, Fatal) handle correctly
- [ ] Message formatting with 0, 1, many arguments works
- [ ] Empty fields, null values, special characters handled gracefully

### Reliability
- [ ] Unit test coverage ≥80%
- [ ] Integration tests pass on Ubuntu and Windows
- [ ] Stress tests with 1000 concurrent connections complete without crash
- [ ] Connection loss and reconnection handled cleanly
- [ ] OTEL Collector unavailability doesn't cause data loss (if buffering enabled)
- [ ] No memory leaks detected (RSS stable over 1 hour under load)

### Performance
- [ ] Parse throughput: ≥1,000 msgs/sec for 100KB payloads
- [ ] OTLP export latency P99: <1 second
- [ ] E2E latency (ADS→OTLP) P95: <200ms
- [ ] Memory usage: <500MB RSS under sustained 10k msgs/sec
- [ ] CPU: <60% single core at 10k msgs/sec

### Compatibility
- [ ] Output byte-identical to .NET service for all 16 object types
- [ ] Runs on Windows 10, 11, Server 2019, 2022 without functional differences
- [ ] Backward compatible: Can consume logs from existing TwinCAT 3.1+ systems

### Operations
- [ ] Configuration validation catches errors before apply
- [ ] All errors logged with actionable context
- [ ] Health check endpoint available (or via signals)
- [ ] Graceful shutdown: In-flight logs exported before exit
- [ ] Prometheus metrics exported for monitoring
- [ ] Startup time: <5 seconds

### Documentation
- [ ] README describes deployment and configuration
- [ ] Example docker-compose.yml provided
- [ ] Troubleshooting guide covers common issues
- [ ] CONTRIBUTING.md describes test running procedures

---

## 8. Continuous Improvement

### Test Result Analysis
- Review test results weekly for flaky tests
- If >5% failure rate on main branch: investigate and fix
- Keep benchmark baselines updated as optimizations applied

### Coverage Growth
- Track coverage trends over time
- Target: +5% coverage per quarter
- Prioritize untested error handling paths

### Performance Monitoring
- Alert if P99 latency degrades >20% from baseline
- Alert if memory usage increases >50MB from baseline
- Quarterly review of throughput improvements


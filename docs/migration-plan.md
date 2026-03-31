# Log4TC Rust Migration Plan

## Executive Summary

This document outlines the comprehensive plan for migrating log4TC from a .NET-based Windows service to a native Rust implementation with OpenTelemetry (OTLP) as the primary output protocol. This is a strategic architectural evolution that simplifies the system while improving performance and maintainability.

**Scope:**
- Replace 19 .NET projects with a single Rust service
- Eliminate 4 output plugins (NLog, Graylog, InfluxDB, SQL) with a unified OTLP exporter
- Maintain binary protocol compatibility with existing TwinCAT PLC library (unchanged)
- Retain feature parity for log ingestion, filtering, and transformation
- Provide seamless transition path for existing deployments

**Timeline:** 10 weeks (5 phases)

**Key Risks:**
- ADS protocol complexity (no existing Rust library)
- Windows service integration challenges
- Performance regressions vs. optimized .NET code
- Data loss during cutover window
- Existing user migrations and backward compatibility

---

## Current State Assessment

### Projects to Replace

The current .NET solution consists of **19 projects**:

**Core Libraries:**
1. Log4Tc.Model - Data models (LogEntry, LogLevel, message structures)
2. Log4Tc.Receiver - ADS protocol listener and binary parser
3. Log4Tc.Dispatcher - Log processing, filtering, and routing pipeline
4. Log4Tc.Output - Base plugin interface and infrastructure
5. Log4Tc.Plugin - Plugin abstraction layer
6. Log4Tc.Utils - Shared utilities and helpers

**Output Plugins (4 to be replaced by OTLP):**
7. Log4Tc.Output.NLog - NLog integration (file, console, email outputs)
8. Log4Tc.Output.Graylog - Graylog GELF protocol exporter
9. Log4Tc.Output.InfluxDb - InfluxDB time-series database exporter
10. Log4Tc.Output.Sql - SQL database exporter (MySQL, PostgreSQL, SQL Server)

**Service & Deployment:**
11. Log4Tc.Service - Windows service host (TopShelf-based)
12. Log4Tc.Setup - WiX installer project

**Testing & Infrastructure:**
13. Log4Tc.Model.Test
14. Log4Tc.Dispatcher.Test
15. Log4Tc.Output.NLog.Test
16. Log4Tc.Output.Graylog.Test
17. Log4Tc.Output.InfluxDb.Test
18. Log4Tc.SmokeTest - Integration tests
19. Log4TcPrototype - Prototype/experimental code

### Output Plugins Being Eliminated

| Plugin | Current Purpose | Replacement Strategy |
|--------|-----------------|----------------------|
| NLog | General-purpose file/console logging | OTLP exporter handles to OTEL Collector, which routes via NLog connector if needed |
| Graylog | GELF protocol direct export | OTLP collector can export to Graylog using native connector |
| InfluxDB | Time-series metrics storage | OpenTelemetry metrics exporter + InfluxDB connector |
| SQL | Relational DB storage (MySQL/Postgres/SQL Server) | OTLP collector with database processors/exporters |

**Key Benefit:** Single OTLP export eliminates plugin complexity; backend flexibility handled by collector configuration.

### TwinCAT PLC Library (Unchanged)

Location: `library/` directory
- Language: IEC 61131-3 (TwinCAT)
- Responsibility: Generate and send log messages via ADS protocol
- Stability: This library remains unchanged; Rust service must maintain binary protocol compatibility

### Existing Users/Deployments

**Considerations for migration:**
- Estimated user base: Small to medium (open-source project, industrial focus)
- Windows-only currently; Rust enables future cross-platform support
- Deployments likely range from single-machine setups to multi-controller centralized logging
- Existing configurations reference plugin-specific settings that will be consolidated
- Some users may have custom plugins; Rust version should provide extensibility mechanism

---

## Migration Phases

### Phase 1: Foundation (Weeks 1-2)

**Objective:** Establish Rust project infrastructure and core data models

**Tasks:**
- Set up Cargo workspace with appropriate crate structure
  - `log4tc-core` - Shared types and utilities
  - `log4tc-receiver` - ADS protocol implementation
  - `log4tc-dispatcher` - Processing pipeline
  - `log4tc-service` - Windows service wrapper
  - `log4tc-exporter` - OTLP exporter
- Define LogEntry struct mirroring .NET model (LogLevel, Message, Arguments, Context)
- Implement configuration system (TOML-based, replacing JSON)
  - Structure matching current settings schema
  - Feature flags for compatibility
- Create CLI utility for local testing and debugging
  - Test configuration loading
  - Validate data model serialization
- Set up CI/CD pipeline (GitHub Actions or Azure Pipelines)
- Establish test infrastructure and conventions

**Deliverable:**
- Compilable Rust workspace
- Core types buildable and serializable
- CLI tool can load and validate configuration
- Build and test pipeline functional
- GitHub repository integration working

**Go/No-Go Criteria:**
- Cargo workspace builds without errors
- Unit tests for core types pass
- Configuration parsing tested with sample configs
- Rust toolchain and dependencies locked

---

### Phase 2: ADS Receiver (Weeks 3-4)

**Objective:** Implement TwinCAT ADS protocol listener and message parser

**Tasks:**
- Implement ADS protocol layer (TCP socket, port 16150)
  - Connection establishment and management
  - ADS header parsing (Command ID, reserved, target/source AMS net, port)
  - Request/response handling
  - State machine for connection lifecycle
- Implement binary protocol v1 parser
  - Support 16 built-in object types (null, byte, word, dword, real, lreal, sint, int, dint, usint, uint, udint, string, bool, ularge, large)
  - Variable-length message parsing
  - Argument and context value deserialization
  - Error handling for malformed messages
- Implement connection management
  - Accept incoming connections
  - Maintain connection state per PLC
  - Handle disconnections and reconnects gracefully
  - Optional: Rate limiting and flood protection
- Create integration tests with mock PLC sender
  - Test all object types
  - Test edge cases (empty strings, boundary values)
  - Test error conditions

**Deliverable:**
- ADS listener operational on port 16150
- Parser handles all 16 object types correctly
- Parsed messages converted to LogEntry structs
- Connection management tested
- Error messages logged appropriately
- Can receive and parse actual TwinCAT messages

**Go/No-Go Criteria:**
- Receiver binds to port 16150 successfully
- Sample log messages from existing .NET system can be parsed
- No message loss under normal load (throughput test)
- Reconnection works after network interruption

---

### Phase 3: OTLP Export (Weeks 5-6)

**Objective:** Implement OpenTelemetry LogRecord export with batching and retry logic

**Tasks:**
- Implement LogEntry -> OTEL LogRecord mapping
  - Severity mapping (Trace, Debug, Info, Warn, Error, Fatal to OTEL SeverityNumber)
  - Timestamp conversion and handling
  - Body and attributes mapping
  - Resource attributes (service name, host, etc.)
  - Optional trace/span correlation if present
- Implement OTLP gRPC exporter
  - gRPC client setup and TLS support
  - Protobuf message serialization
  - Configurable exporter endpoint
  - HTTP/2 fallback option
- Implement HTTP exporter variant
  - JSON serialization path
  - Fallback if gRPC unavailable
- Implement batching logic
  - Configurable batch size (default 100, max 1000)
  - Flush timeout (default 1 second)
  - In-memory queue with overflow handling
- Implement retry logic
  - Exponential backoff (initial 100ms, max 30s)
  - Configurable retry count (default 3)
  - Distinguish retryable vs. fatal errors
  - Dead letter queue for failed messages
- Integration with OTEL Collector
  - Test end-to-end with local collector instance
  - Validate message format correctness
- Error handling and monitoring
  - Log exporter health status
  - Metrics for messages sent, dropped, retried
  - Alerts for persistent export failures

**Deliverable:**
- LogEntry -> OTEL LogRecord conversion working
- gRPC and HTTP exporters functional
- Batching and buffering operational
- Retry mechanism tested with failure scenarios
- End-to-end flow from PLC to OTEL Collector verified
- Configuration examples for common backends

**Go/No-Go Criteria:**
- Messages successfully export to sample OTEL Collector
- Batch behavior verified (correct size and timing)
- Retry logic tested (simulate collector downtime)
- No message loss under normal operations
- Export latency under 500ms p99

---

### Phase 4: Containerization & Production (Weeks 7-8)

**Objective:** Create production-ready containerized service with monitoring and multi-platform deployment

**Tasks:**
- Containerization & platform support
  - Docker image with minimal base (debian:bookworm-slim)
  - docker-compose orchestration with OTEL Collector
  - Linux systemd unit file for traditional deployments
  - Standalone binary packaging (no OS dependencies)
  - Cross-platform testing (Windows, Linux, macOS)
- Installer and packaging
  - Standalone executable distribution
  - Docker Hub image publishing
  - Release artifacts for each platform
  - Configuration migration tool from .NET version
- Configuration management
  - TOML config file migration from existing JSON
  - Config validation on startup
  - Hot-reload capability (optional, phase 2 improvement)
  - Default configuration templates
- Monitoring and health checks
  - Health check endpoint (HTTP)
  - Metrics collection (messages processed, dropped, latency)
  - Structured logging for service events
  - Prometheus metrics export
- Performance optimization
  - Benchmarking against .NET baseline
  - Memory profiling and optimization
  - CPU utilization analysis
  - Network throughput optimization
- Production hardening
  - Graceful shutdown with timeout
  - Message queue persistence on shutdown
  - Configuration backups
  - Comprehensive error handling
  - Security review (ADS protocol, port binding, file permissions)

**Deliverable:**
- Docker image published to Docker Hub
- docker-compose configuration for full stack deployment
- Linux systemd unit file for traditional deployments
- Standalone executables for Windows, Linux, macOS
- Monitoring dashboards and metrics available
- Config migration tool for existing deployments
- Comprehensive error logging
- Performance parity or better vs. .NET version
- Release candidate ready for testing

**Go/No-Go Criteria:**
- Service installs and starts successfully
- Service recovers from crashes
- Messages processed under load (1000 msg/sec)
- Memory stable after 8-hour run
- CPU usage <5% idle, <30% at peak load
- Zero message loss on graceful shutdown

---

### Phase 5: Validation & Cutover (Weeks 9-10)

**Objective:** Parallel testing, comparison validation, and production deployment

**Tasks:**
- Parallel run setup
  - Install Rust service alongside .NET service
  - Configure both to receive from same PLC (requires ADS multiplexing or dual connection)
  - Run in production environment for 1-2 weeks
- Comparison testing
  - Log message count matching
  - Message field validation (timestamps, levels, content)
  - OTLP output quality vs. old plugins
  - Latency and throughput comparison
  - Error rate comparison
  - Resource usage monitoring
- Configuration migration tooling
  - Script to convert JSON plugin configs to TOML OTLP config
  - Validation of migrated configs
  - Documentation for manual adjustments
- End-user documentation
  - Installation guide for Rust version
  - Configuration guide (TOML format)
  - Troubleshooting guide
  - Breaking changes document
  - OTLP backend setup examples (Graylog, InfluxDB, databases via collector)
  - FAQ for users on .NET version
- Cutover procedure development
  - Rollback plan (detailed in Risk Register)
  - Data continuity strategy
  - Backup and recovery procedures
  - Communication plan for users
  - Cutover checklist
- Final testing
  - Smoke test suite on production config
  - Load testing at 2x expected peak load
  - Long-soak test (24+ hours)
  - Failure scenario testing
- Production deployment
  - Staged rollout (canary -> broader deployment)
  - Monitoring and alert setup
  - Support team training
  - User communication and timeline

**Deliverable:**
- Parallel run completed with validation report
- Configuration migration tool tested
- End-user documentation complete
- Cutover procedure documented and approved
- Production deployment plan finalized
- Support procedures established

**Go/No-Go Criteria:**
- Message counts match within 0.1% between systems
- No field validation errors on sample of 10,000+ messages
- OTLP export quality equivalent to plugin outputs
- Rust version uses <75% of .NET version's memory
- Latency acceptable to end users
- All comparison tests pass
- Stakeholder sign-off obtained

---

## Dependency Graph

```
Phase 1: Foundation
    ├─ Cargo workspace setup
    ├─ Core data models
    ├─ Configuration system
    └─ CLI testing tool
           |
           V
Phase 2: ADS Receiver (depends on Phase 1)
    ├─ ADS protocol layer (socket handling)
    ├─ Binary protocol parser (depends on core models)
    ├─ Connection management
    └─ Integration tests (depends on protocol layer)
           |
           V
Phase 3: OTLP Export (depends on Phase 1, 2)
    ├─ LogEntry -> OTEL mapping (depends on core models)
    ├─ gRPC exporter (depends on core models, endpoint config)
    ├─ HTTP exporter (depends on gRPC exporter)
    ├─ Batching (depends on model definitions)
    ├─ Retry logic (depends on exporter implementations)
    └─ Integration tests (depends on all above)
           |
           V
Phase 4: Containerization & Production (depends on Phase 3)
    ├─ Docker image (depends on complete service)
    ├─ docker-compose orchestration (depends on Docker image)
    ├─ systemd unit file (depends on complete service)
    ├─ Configuration management (depends on Phase 1 config)
    ├─ Monitoring (can be parallel with other tasks)
    ├─ Performance optimization (depends on complete receiver + exporter)
    └─ Production hardening (depends on complete service)
           |
           V
Phase 5: Validation & Cutover (depends on Phase 4)
    ├─ Parallel run setup (depends on Phase 4 release)
    ├─ Comparison testing (depends on parallel run)
    ├─ Configuration migration (depends on Phase 1 config system)
    ├─ Documentation (can start in Phase 4)
    ├─ User communication (depends on validated migration plan)
    └─ Production deployment (depends on all validation)
```

**Parallelization Opportunities:**
- Phase 1 and Phase 2 design can overlap (weeks 1-2 design, 3-4 implement)
- Configuration migration tool development can start in Phase 1
- Documentation drafting can start in Phase 3 (while service is functional)
- Support team training materials preparation in Phase 4
- Installer development can begin in Phase 3 (basic MSI scaffolding)

---

## Risk Register

| Risk | Probability | Impact | Mitigation Strategy |
|------|-------------|--------|---------------------|
| **ADS Protocol Complexity** | High | High | Protocol reverse-engineering completed in Phase 2; extensive testing with real PLC messages; create protocol spec document; consider protocol library extraction for reuse |
| **Cross-Platform Compatibility** | Medium | High | Test on Windows, Linux, macOS; containerize for consistency; use platform-agnostic Rust code; monitor platform-specific issues |
| **Performance Regression** | Medium | High | Establish performance baseline with .NET version in week 1; benchmark each phase against baseline; optimize hot paths; consider JIT compilation options if needed |
| **Data Loss During Migration** | Medium | High | Implement persistent message queue in Phase 4; graceful shutdown with timeout; dual-system parallel run in Phase 5; test failover scenarios extensively |
| **Rust Ecosystem Immaturity** | Low | Medium | Lock dependencies in Cargo.lock; vendor critical crates; monitor security advisories; plan maintenance strategy for unmaintained dependencies |
| **Configuration Breaking Changes** | Medium | Medium | Develop config migration tool in Phase 5; provide mapping documentation; support both old and new config formats in transition period; clear changelog |
| **OTLP Collector Availability** | Low | High | Implement robust retry logic (Phase 3); support multiple collector endpoints for failover; implement local message queue; health monitoring |
| **Existing User Migration Issues** | Medium | Medium | Create detailed migration guide and troubleshooting docs; provide config validation tool; offer transition support period (3-6 months); maintain .NET version in parallel initially |
| **Port 16150 Conflicts** | Low | Medium | Implement configurable port in TOML config; check for port availability on startup; document setup procedures; implement port conflict detection |
| **Memory Leak in Rust Service** | Low | Medium | Implement memory profiling in Phase 4; add memory monitoring alerts; use tools like Valgrind/Dr. Memory; regular memory audits; memory limit enforcement |
| **Backward Compatibility Issues** | Medium | High | Maintain binary protocol v1 support indefinitely; create compatibility layer; version protocol negotiation in Phase 2; extensive regression testing |
| **OTLP Format Incompatibility** | Low | Medium | Follow OpenTelemetry spec strictly; validate against reference implementations; test with multiple collector versions; maintain compatibility matrix in docs |
| **SSL/TLS Certificate Issues** | Low | Medium | Support both secure and insecure gRPC connections; provide clear certificate setup guide; implement certificate validation fallback; monitor certificate expiry |
| **Windows Registry/Permissions Issues** | Low | Medium | Run installer with elevated privileges; validate registry writes; implement registry cleanup on uninstall; test on restricted user accounts |

**Contingency Plans:**
1. **Protocol Complexity:** Extract ADS protocol library as separate Rust crate for community contribution
2. **Performance Issues:** Implement optional C++ extension via FFI for critical hot paths
3. **Schedule Slippage:** Reduce Phase 5 parallel run duration from 2 weeks to 1 week if needed (increase Phase 5 load testing)
4. **Migration Resistance:** Extend .NET version support by 6 months; maintain both systems in parallel longer

---

## Feature Parity Checklist

| Feature | Current Implementation | Rust Implementation | Status | Notes |
|---------|------------------------|---------------------|--------|-------|
| **ADS Protocol** | .NET ADS library | Custom Rust implementation | Phase 2 | Must maintain binary compatibility |
| **16 Object Types** | Reflection-based deserialization | Custom parsers per type | Phase 2 | Full type support required |
| **Message Formatting** | String interpolation, templates | Message template engine (tera or custom) | Phase 1 | Must support structured logging syntax |
| **Log Levels** | Trace, Debug, Info, Warn, Error, Fatal | OTEL SeverityNumber mapping | Phase 1 | Bidirectional mapping to OTEL |
| **Context Properties** | Hierarchical context stack | AsyncLocal equivalent (thread-local) | Phase 1 | Scope-based context management |
| **Arguments Logging** | Variable-length argument serialization | Struct field serialization | Phase 1 | Support all 16 types |
| **Filtering** | Plugin-based filtering | Configuration-based filtering | Phase 3 | Regex and expression-based rules |
| **Performance Metrics** | Limited built-in metrics | Prometheus metrics integration | Phase 4 | Enhanced observability |
| **Configuration** | JSON plugin configs | TOML centralized config | Phase 1 | Consolidate all plugin configs |
| **Output Plugins** | NLog, Graylog, InfluxDB, SQL | OTLP single exporter | Phase 3 | Collector handles backend routing |
| **Deployment** | Windows Service only | Docker, systemd, standalone binary | Phase 4 | Multi-platform support |
| **Packaging** | MSI installer | Docker images, native executables | Phase 4 | Platform-specific distributions |
| **Hot-reload Config** | Limited | Optional in Phase 2 | Phase 2+ | Could be added for flexibility |
| **Custom Plugins** | Plugin interface (.NET DLL) | No direct equivalent | Future | Community feedback determines approach |
| **Multi-PLC Support** | Multiple connections | Native support (concurrent) | Phase 2 | Rust async naturally supports many connections |
| **TwinCAT Library** | IEC 61131-3 | Unchanged | - | No changes required |

---

## Rollback Strategy

### Pre-Cutover Rollback (Phase 5 During Testing)
1. Stop Rust service
2. Disable OTLP export configuration
3. Restart .NET service (already running in parallel)
4. Data continuity: No data loss (both systems running in parallel)

### Post-Cutover Rollback (If Issues Found in Production)

**Immediate (First 48 Hours):**
1. **Automated Alert Trigger:** Monitor detects excessive error rate (>1% message loss) or service instability
2. **Manual Activation:** Operator decision to rollback based on monitoring/user reports
3. **Activation Procedure:**
   ```
   Step 1: Stop Rust service (systemctl stop log4tc-rust)
   Step 2: Restart .NET service (net start Log4TcService)
   Step 3: Verify message flow on .NET service (check event log)
   Step 4: Monitor metrics for 30 minutes
   Step 5: Confirm data continuity (compare message counts)
   ```
4. **Time to Rollback:** 5-15 minutes
5. **Data Loss Risk:** None (OTEL Collector queues messages during transition)

**Extended Rollback (Days 2-30):**
- Maintain Rust service binary and config for future re-deployment
- Plan bug fixes and retest in Phase 4 environment
- Schedule re-cutover with additional validation
- Continue .NET service operation as stable fallback

### Data Preservation Approaches

**Strategy 1: Dual-Write (Recommended)**
- Implement in Phase 4: Write to both OTLP and persistent local queue
- On service stop, messages in queue survive restart
- Retry queue on service restart sends queued messages
- Ensures zero-loss during transitions

**Strategy 2: Collector-Side Buffering**
- Rely on OTEL Collector's persistence features
- Configure collector to queue messages to disk
- Requires stable OTEL Collector deployment
- Risk: Collector not available during transition

**Strategy 3: Database Persistence (Fallback)**
- Optional Phase 4 feature: Local SQLite queue
- Only used if OTLP export fails
- Periodically flush queue to OTLP when collector recovers
- Adds operational complexity

### Testing the Rollback

In Phase 5:
1. Run parallel system for 1 week
2. Perform rollback drill:
   - Stop Rust service cleanly
   - Verify .NET service continues without interruption
   - Restart Rust service
   - Compare message counts from all three periods
3. Repeat rollback drill 3x to validate procedure
4. Document any timing issues or data gaps discovered

### User Communication

**Before Cutover:**
- Email: "Log4TC Service Update Scheduled"
- Include rollback conditions and timeline
- Publish rollback procedure for transparency

**If Rollback Occurs:**
- Immediate notification: "Log4TC rolled back to previous version"
- Expected timeline for re-cutover
- Investigation findings and lessons learned
- No action required from users

---

## Go/No-Go Criteria Summary

### Phase 1 Go/No-Go
- [ ] Cargo workspace builds with `cargo build --release` (zero warnings preferred)
- [ ] Core types serialize/deserialize correctly (unit tests passing)
- [ ] Configuration system loads sample configs without errors
- [ ] CI/CD pipeline executes on each commit
- [ ] Test suite coverage >80% for core crate

### Phase 2 Go/No-Go
- [ ] ADS receiver binds to port 16150 and accepts connections
- [ ] All 16 object types parse correctly (verified with test vectors)
- [ ] Sample logs from existing .NET system parse without errors
- [ ] Zero message loss under 1000 messages/second load
- [ ] Receiver recovers from network disconnection within 30 seconds
- [ ] Error conditions produce appropriate logs (no panics)

### Phase 3 Go/No-Go
- [ ] LogEntry maps to OTEL LogRecord with all fields preserved
- [ ] Messages export to OTEL Collector successfully (verified via collector logs)
- [ ] Batching works (configurable size, respect timeout)
- [ ] Retry mechanism activates and eventually succeeds (tested with collector downtime)
- [ ] No message loss under normal conditions (verified with counter checks)
- [ ] Export latency <500ms p99 for typical message sizes

### Phase 4 Go/No-Go
- [ ] Windows service installs with administrator privilege
- [ ] Service starts automatically on Windows boot
- [ ] Service handles CTRL_C and service stop signals gracefully
- [ ] Configuration migration from .NET version succeeds (test on 3+ real configs)
- [ ] Performance: Memory usage <150MB, CPU <30% under 1000 msg/sec
- [ ] Service can process 1000+ messages/second without message loss
- [ ] Service recovers automatically from unexpected crashes

### Phase 5 Go/No-Go
- [ ] Parallel run: Both systems running for 7+ days without issues
- [ ] Message counts match within 0.1% (difference <1 message in 1000)
- [ ] Message field validation: Zero discrepancies on sample of 10,000+ messages
- [ ] Configuration migration tool tested on 10+ real user configs
- [ ] End-user documentation reviewed and approved
- [ ] All stakeholders (developers, ops, users) sign off on cutover plan
- [ ] Rollback procedure tested successfully 3 times
- [ ] Support team trained and confidence level high

---

## Resource Requirements

### Team Composition

**Full-Time (10 weeks):**
- 1x Rust Backend Engineer (primary development)
- 1x Windows Systems Engineer (service integration, installer, testing)
- 0.5x DevOps Engineer (CI/CD setup, testing infrastructure)

**Part-Time (as needed):**
- 1x Project Manager (coordination, stakeholder communication)
- 1x Technical Writer (documentation in Phases 4-5)
- 1x QA Engineer (Phase 5 validation testing)

**Optional:**
- 1x Security Engineer (Phase 4 security review)
- 1x Performance Engineer (Phase 4-5 optimization)

### Skills Required

**Must-Have:**
- Rust language (async/await, error handling, ownership model)
- Windows service development
- Network protocol implementation (TCP/IP, binary parsing)
- Testing methodologies (unit, integration, load testing)
- Git and version control

**Nice-to-Have:**
- OpenTelemetry specification knowledge
- gRPC and Protobuf
- .NET architecture understanding (for migration context)
- TwinCAT/IEC 61131-3 familiarity
- Configuration management (TOML, schema validation)
- Windows installer creation (MSI/WiX)

### Tooling & Infrastructure

**Development:**
- Rust toolchain (stable channel, 1.70+)
- VS Code with rust-analyzer extension
- Cargo (dependency management)
- Git and GitHub
- Optional: IntelliJ IDEA with Rust plugin

**Testing:**
- Local OTEL Collector instance (Docker or local installation)
- Windows Server VMs for service testing (2-3 instances)
- Load testing tool (wrk, Apache JMeter, or custom)
- Memory profiling tools (Valgrind, Dr. Memory)
- Network simulation tools (tc, clumsy, or similar)

**Build & Deployment:**
- GitHub Actions or Azure Pipelines
- Docker for OTEL Collector testing
- MSI creation tool (WiX or cargo-wix)
- Code signing certificate (for production installer)
- Artifact repository (GitHub Releases or Azure Artifacts)

**Documentation:**
- Markdown editor (VS Code)
- Diagram tool (Mermaid, PlantUML, or Draw.io)
- API documentation generation (rustdoc)

### Dependencies (Key Crates)

**Phase 1:**
- `tokio` - async runtime
- `serde` / `toml` - configuration
- `log` / `tracing` - logging
- `anyhow` - error handling

**Phase 2:**
- `tokio::net` - TCP networking
- `bytes` / `byteorder` - binary parsing
- No external ADS crate (custom implementation)

**Phase 3:**
- `grpc` / `tonic` - gRPC client
- `protobuf` / `prost` - Protobuf serialization
- `opentelemetry` - OTEL API
- `opentelemetry-proto` - OTEL Protobuf definitions

**Phase 4:**
- `windows-service` - Windows service integration
- `windows` - Windows API
- `prometheus` - Metrics export

**Testing & Dev:**
- `tokio-test` - async testing
- `criterion` - benchmarking
- `proptest` - property-based testing
- `mockall` / `mocktail` - mocking

### Development Environment Setup

Estimated effort: 2-4 hours per developer

```bash
# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Clone repository
git clone https://github.com/mbc-engineering/log4tc.git
cd log4tc

# Install dependencies (handled by Cargo)
cargo build

# Set up local OTEL Collector for testing
docker run -d -p 4317:4317 -p 4318:4318 \
  otel/opentelemetry-collector:latest
```

### Estimated Timeline by Phase (Including Overhead)

| Phase | Development | Testing | Contingency | Total |
|-------|-------------|---------|-------------|-------|
| 1: Foundation | 3 days | 1 day | 1 day | 5 days (1 week) |
| 2: ADS Receiver | 5 days | 3 days | 2 days | 10 days (2 weeks) |
| 3: OTLP Export | 5 days | 3 days | 2 days | 10 days (2 weeks) |
| 4: Service & Production | 4 days | 3 days | 3 days | 10 days (2 weeks) |
| 5: Validation & Cutover | 2 days | 8 days | 2 days | 12 days (2.4 weeks, overlaps with Phase 4) |
| **Total** | **19 days** | **18 days** | **10 days** | **47 days (9.4 weeks)** |

*Note: Timeline assumes no major blockers and weekly coordination meetings. Adjust for team part-time availability.*

---

## Success Metrics

### Performance Metrics

| Metric | Target | Phase | Measurement |
|--------|--------|-------|-------------|
| Message Throughput | ≥ 2000 msg/sec | Phase 4 | Load test with synthetic messages |
| Message Latency (p50) | ≤ 50ms | Phase 3 | End-to-end: PLC → OTEL Collector |
| Message Latency (p99) | ≤ 500ms | Phase 3 | Including batching and retry delays |
| Memory Usage | < 150 MB idle, < 250 MB @ 1000 msg/sec | Phase 4 | Monitor for 8+ hours under load |
| CPU Usage | < 5% idle, < 30% @ 1000 msg/sec | Phase 4 | Monitor for 8+ hours under load |
| Message Loss Rate | 0% (under normal operations) | Phase 3 | Counter verification after 10k messages |
| Service Recovery Time | < 30 seconds | Phase 2 | After network disconnect |
| Startup Time | < 10 seconds | Phase 4 | From service start to accepting connections |

### Reliability Metrics

| Metric | Target | Phase | Measurement |
|--------|--------|-------|-------------|
| Uptime | ≥ 99.5% | Phase 5 | Parallel run period |
| Message Delivery Accuracy | 100% (field-by-field) | Phase 5 | Compare 100k+ messages with .NET version |
| OTLP Export Success Rate | ≥ 99.9% | Phase 3 | Messages reaching OTEL Collector |
| Connection Stability | No unexpected disconnects | Phase 2 | 7-day soak test |
| Config Migration Success Rate | 100% | Phase 5 | Tool succeeds on all test configs |

### User Experience Metrics

| Metric | Target | Phase |
|--------|--------|-------|
| Time to Install | < 10 minutes (including config) | Phase 4 |
| Installation Failure Rate | 0% | Phase 5 |
| Configuration Validation Errors | 0% on migrated configs | Phase 5 |
| Documentation Completeness | 100% (all features covered) | Phase 5 |
| User Support Tickets (first month) | < 5 | Phase 5+ |

### Migration Success Metrics

| Metric | Target | Notes |
|--------|--------|-------|
| Existing Deployments Migrated | 100% (or documented alternative) | Phase 5+ |
| Parallel Run Duration | 7-14 days | Confidence building |
| Parallel Run Issues | < 3 significant issues | Indicates stability |
| Go/No-Go Votes (Stakeholders) | 100% approval | Final gate for Phase 5 |
| Rollback Activation Events | 0 (or <1 after first week) | Indicates production readiness |

---

## Appendices

### A. ADS Protocol Specification (Reference)

The ADS protocol (TwinCAT Automation Device Specification) runs on TCP/IP, typically port 16150.

**Key aspects to implement:**
- AMS (Automation Message Service) header format
- Command/Response message types for logging
- 16 variable types and their binary encodings
- Connection state machine
- Error handling and status codes

**Resources:**
- Beckhoff ADS Protocol specification (available from Beckhoff)
- Reference implementation in existing Log4Tc.Receiver (.NET)
- TwinCAT SDK documentation

### B. Configuration Schema (TOML)

```toml
[service]
name = "log4tc"
description = "TwinCAT Log Service"
listen_port = 16150
max_connections = 100

[logging]
level = "info"
format = "json"
output = "eventlog"

[exporter]
type = "otlp"
protocol = "grpc"  # or "http"
endpoint = "http://otel-collector:4317"
batch_size = 100
batch_timeout_ms = 1000
export_timeout_ms = 30000

[exporter.retry]
enabled = true
initial_backoff_ms = 100
max_backoff_ms = 30000
max_retries = 3

[filtering]
# Optional: Filter messages before export
patterns = []  # Regex patterns to match

[monitoring]
metrics_enabled = true
metrics_port = 8888
health_check_enabled = true
health_check_port = 8889
```

### C. Security Considerations

**Port Binding:**
- Port 16150 should be restricted to local network (firewall rules)
- Consider VPN/tunnel for remote PLC connections

**OTLP Export:**
- Support mTLS for gRPC endpoint
- Validate collector certificate
- Support authentication tokens

**Configuration:**
- Restrict config file permissions (600 on Unix, read-only on Windows)
- Avoid embedding secrets in TOML (support environment variables)

**Windows Service:**
- Run with least-privilege account (not SYSTEM)
- Implement UAC prompt for installation
- Code signing for installer

### D. Monitoring and Alerting Recommendations

**Key Metrics to Monitor:**
- ADS connection count and health
- Messages received vs. messages exported (detect drops)
- Export latency (p50, p95, p99)
- OTLP collector availability
- Service resource usage (memory, CPU, disk)

**Recommended Alerts:**
- Connection drop (>10 sec without messages)
- Export error rate > 1%
- Memory usage > 200 MB
- Service restart loop detected
- Queue depth > 10,000 messages

**Tools:**
- Prometheus for metrics collection
- Grafana for dashboards
- AlertManager for alerting

---

## Document History

| Version | Date | Author | Changes |
|---------|------|--------|---------|
| 1.0 | 2025-03-31 | Architecture Team | Initial comprehensive migration plan |

---

## Approval Sign-Offs

| Role | Name | Date | Signature |
|------|------|------|-----------|
| Project Manager | | | |
| Technical Lead | | | |
| Operations | | | |
| Product Owner | | | |
| Release Manager | | | |

---

## Related Documentation

- [Current Architecture Analysis](./internal/description.md)
- [Phase 1: Foundation Detailed Design](./phase-1-design.md) (TBD)
- [Phase 2: ADS Protocol Specification](./phase-2-ads-spec.md) (TBD)
- [Configuration Migration Guide](./config-migration.md) (TBD)
- [OpenTelemetry Integration Guide](./otel-integration.md) (TBD)
- [Operator Runbook](./runbook.md) (TBD)

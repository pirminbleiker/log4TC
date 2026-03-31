# Migration Plan & Roadmap - Log4TC .NET to Rust

**Document Version**: 1.0  
**Target Date**: Q3 2026  
**Last Updated**: March 31, 2026

---

## Executive Summary

This document outlines the phased migration strategy from the .NET-based Log4TC service to the Rust-based implementation. The migration maintains backward compatibility via ADS protocol support while introducing the modern OTEL protocol as the primary interface.

---

## Migration Phases

### Phase 1: Foundation (COMPLETE ✅)
**Duration**: 4 weeks  
**Goal**: Establish Rust codebase with core functionality

**Deliverables**:
- ✅ Workspace setup (log4tc-core, log4tc-ads, log4tc-otel, log4tc-service)
- ✅ Core models (LogEntry, LogRecord, LogLevel)
- ✅ ADS protocol parser with security limits
- ✅ OTEL HTTP receiver and exporter
- ✅ Dispatcher and channel-based routing
- ✅ Configuration system
- ✅ Basic error handling

**Success Criteria**:
- All modules compile without warnings
- Unit tests pass (>90% code coverage)
- No unsafe code (except bytes reading)
- Rust edition 2021, MSRV 1.70+

### Phase 2: Completion (CURRENT)
**Duration**: 3 weeks  
**Goal**: Feature parity with .NET implementation

**Deliverables**:
- [ ] Output plugin system (NLog, Graylog, InfluxDB, SQL)
- [ ] Message template formatting with arguments
- [ ] Graceful shutdown with timeouts
- [ ] Connection pooling for outputs
- [ ] Comprehensive integration tests
- [ ] Performance benchmarks
- [ ] Documentation (8 documents)

**Success Criteria**:
- All .NET features replicated
- 10k+ msgs/sec throughput
- <2ms p99 latency
- <100MB memory baseline
- Integration tests pass

### Phase 3: Production Hardening (PLANNED)
**Duration**: 3 weeks  
**Goal**: Prepare for production deployment

**Deliverables**:
- [ ] Windows service integration (SC.exe commands)
- [ ] Security hardening (TLS, API keys, rate limiting)
- [ ] Monitoring and health checks
- [ ] Alerting and metrics collection
- [ ] CI/CD pipeline (GitHub Actions)
- [ ] Load testing (spike/endurance tests)
- [ ] Security audit and penetration testing

**Success Criteria**:
- Service installs/uninstalls cleanly
- OWASP Top 10 security review passed
- <1% error rate under load
- Health checks respond in <100ms
- All logs exported successfully

### Phase 4: Enhancement (POST-LAUNCH)
**Duration**: Ongoing  
**Goal**: Extended features and optimization

**Deliverables**:
- [ ] Additional output plugins (Elasticsearch, Splunk, etc.)
- [ ] gRPC receiver (for native gRPC clients)
- [ ] Hot-reload configuration
- [ ] Distributed tracing support
- [ ] Custom transforms/enrichment pipeline
- [ ] Dashboard templates for Grafana

---

## Parallelization Opportunities

```
Phase 1:
  Crate A (core)          Crate B (ADS)           Crate C (OTEL)
       │                       │                        │
       └──────────────────────┴────────────────────────┘
                              │
                         Service (waits for A, B, C)

Phase 2:
  Output plugins (4 plugins can be implemented in parallel)
       │        │        │        │
       ├────────┼────────┼────────┤
       │        │        │        │
    NLog   Graylog   InfluxDB   SQL

Phase 3:
  Testing (unit/integration/perf can run in parallel)
       │        │        │
       ├────────┼────────┤
       │        │        │
    Unit    Integration  Perf
```

---

## Dependencies

### Blocking Dependencies

```
Phase 1:
  log4tc-core must complete before:
    - log4tc-ads (needs LogLevel, LogEntry)
    - log4tc-otel (needs LogLevel, LogEntry)

  ADS and OTEL can proceed in parallel after core

Phase 2:
  Output plugins depend on:
    - log4tc-service (dispatcher interface)
    - Not blocked by each other

Phase 3:
  Testing depends on:
    - All Phase 2 deliverables complete
    - Stable API

Phase 4:
  Enhancement features don't block releases
    - Can proceed in parallel
    - Staged rollout possible
```

### External Dependencies

```
Rust ecosystem (all available):
  - tokio (async runtime)
  - axum (HTTP framework)
  - reqwest (HTTP client)
  - serde (serialization)
  - chrono (timestamps)

Not blocking:
  - OTEL collector (can mock for testing)
  - TwinCAT PLC (can simulate with test client)
  - Output backends (can mock HTTP/UDP responses)
```

---

## Risk Analysis

### High Risks

**Risk**: Rust async issues (deadlocks, panics)  
**Probability**: Medium  
**Impact**: Service crashes  
**Mitigation**: 
- Extensive testing in Phase 2
- Use tracing/logging to catch panics early
- Test concurrent scenarios (100+ connections)

**Risk**: Performance regression vs .NET  
**Probability**: Low  
**Impact**: Can't meet 10k msgs/sec target  
**Mitigation**:
- Benchmark during Phase 1
- Profile early and often
- Consider zero-copy optimizations if needed

**Risk**: ADS protocol incompatibility with legacy PLCs  
**Probability**: Low  
**Impact**: Legacy systems can't upgrade  
**Mitigation**:
- Test with actual TwinCAT versions (v3.1, v4.0)
- Maintain backwards compatibility
- Document version matrix

### Medium Risks

**Risk**: Output plugin implementation delays  
**Probability**: Medium  
**Impact**: Feature parity delayed  
**Mitigation**:
- Start with most critical plugins (OTEL, NLog)
- Use mocks for untested backends
- Parallelize plugin development

**Risk**: Configuration format change  
**Probability**: Medium  
**Impact**: Migration tool needed  
**Mitigation**:
- Support both JSON and TOML
- Provide migration script (.NET → Rust)
- Document changes clearly

**Risk**: TLS/certificate issues in Windows service  
**Probability**: Medium  
**Impact**: Can't export to secure collectors  
**Mitigation**:
- Early testing with HTTPS endpoints
- Clear documentation for certificate setup
- Support custom CA certificates

### Low Risks

**Risk**: Rust compiler update breaks build  
**Probability**: Low  
**Impact**: Maintenance burden  
**Mitigation**:
- Pin Rust edition to 2021
- Use stable channel only
- Document MSRV (1.70+)

---

## Feature Parity Checklist

### Receivers

```
Feature                 .NET        Rust        Status
───────────────────────────────────────────────────────
ADS TCP on 16150       ✅          ✅          COMPLETE
Message parsing         ✅          ✅          COMPLETE
Connection limit        ✅          ✅          COMPLETE
Timeout handling        ✅          ✅          COMPLETE
Error responses (ACK/NAK) ✅        ✅          COMPLETE
```

### Processing

```
Feature                 .NET        Rust        Status
───────────────────────────────────────────────────────
LogEntry model          ✅          ✅          COMPLETE
LogLevel enum           ✅          ✅          COMPLETE
Message formatting      ⏳          ⏳          IN PROGRESS
Template arguments      ✅          ⏳          IN PROGRESS
Context variables       ✅          ✅          COMPLETE
Channel queuing         ✅          ✅          COMPLETE
Dispatcher routing      ✅          ✅          COMPLETE
```

### Output Plugins

```
Plugin                  .NET        Rust        Status
───────────────────────────────────────────────────────
NLog (HTTP)             ✅          ⏳          IN PROGRESS
Graylog (GELF/UDP)      ✅          ⏳          PLANNED
InfluxDB (HTTP)         ✅          ⏳          PLANNED
SQL (ODBC)              ✅          ⏳          PLANNED
OTEL (primary)          ❌          ✅          NEW
```

### Configuration

```
Feature                 .NET        Rust        Status
───────────────────────────────────────────────────────
JSON config             ✅          ✅          COMPLETE
TOML config             ❌          ✅          ENHANCED
Env var override        ✅          ✅          COMPLETE
Hot reload              ❌          ⏳          PLANNED
Validation              ✅          ✅          COMPLETE
Default values          ✅          ✅          COMPLETE
```

### Service Management

```
Feature                 .NET        Rust        Status
───────────────────────────────────────────────────────
Windows service         ✅          ⏳          IN PROGRESS
Graceful shutdown       ✅          ✅          COMPLETE
Timeout handling        ✅          ✅          COMPLETE
Error logging           ✅          ✅          COMPLETE
Systemd unit            ❌          ⏳          PLANNED
Docker image            ❌          ⏳          PLANNED
```

### Monitoring/Operations

```
Feature                 .NET        Rust        Status
───────────────────────────────────────────────────────
Event logging           ✅          ⏳          IN PROGRESS
Health checks           ✅          ⏳          PLANNED
Metrics export          ❌          ⏳          PLANNED
Structured logging      ❌          ✅          ENHANCED
Tracing support         ❌          ✅          ENHANCED
```

---

## Go/No-Go Criteria Per Phase

### Phase 1 Gate

**Go Criteria**:
- [ ] All 4 crates compile with no warnings
- [ ] Unit tests pass (>95% code coverage)
- [ ] Manual ADS protocol tests pass
- [ ] Manual OTEL HTTP tests pass
- [ ] No panic/deadlock issues found
- [ ] Memory leaks checked (valgrind/heaptrack)

**No-Go Triggers**:
- [ ] Protocol parser fails >5% of test cases
- [ ] Async runtime deadlocks
- [ ] Memory usage >500MB baseline
- [ ] Unresolvable Rust compilation issues

### Phase 2 Gate

**Go Criteria**:
- [ ] All output plugins implemented and tested
- [ ] 10k+ msgs/sec throughput achieved
- [ ] <2ms p99 latency measured
- [ ] <100MB memory baseline confirmed
- [ ] Integration tests pass (>90% coverage)
- [ ] Feature parity checklist complete (100%)
- [ ] Security review passed (all critical findings fixed)

**No-Go Triggers**:
- [ ] Performance <5k msgs/sec
- [ ] Latency >10ms p99
- [ ] Memory >200MB baseline
- [ ] <80% feature parity
- [ ] Security vulnerabilities found

### Phase 3 Gate

**Go Criteria**:
- [ ] Load testing passed (sustained 10k msgs/sec)
- [ ] Spike testing passed (no crashes at 20k msgs/sec)
- [ ] Windows service installation tested
- [ ] TLS/HTTPS fully functional
- [ ] OWASP Top 10 audit passed
- [ ] Penetration testing cleared
- [ ] Documentation complete and reviewed
- [ ] Rollback procedure tested

**No-Go Triggers**:
- [ ] Load test failure (crashes, data loss)
- [ ] Service installation fails
- [ ] Security audit findings not resolved
- [ ] Documentation incomplete or unclear

---

## Migration Timeline

```
Week    Phase 1         Phase 2         Phase 3         Phase 4
────────────────────────────────────────────────────────────────
1-2     Core impl       (wait)          (wait)          (wait)
3-4     Testing         Start output    (wait)          (wait)
5-6     Code review     Plugin impl     (wait)          (wait)
7-8     (complete)      Testing         Start hardening (wait)
9-10    (complete)      (complete)      Testing         Enhancement
11-12   (complete)      (complete)      (complete)      Enhancement

Actual dates (if starting now):
W1-2:   Apr 1-14        (Foundation)
W3-4:   Apr 15-28       (Core testing)
W5-6:   Apr 29-May 12   (Output plugins)
W7-8:   May 13-26       (Plugin testing)
W9-10:  May 27-Jun 9    (Hardening)
W11-12: Jun 10-23       (Production ready)
```

---

## Rollback Strategy

### Pre-Deployment

1. **Keep .NET service running** in parallel for 2-4 weeks
2. **Run both services** simultaneously (or round-robin traffic)
3. **Compare outputs** (log entry counts, formats)
4. **Validate no data loss** in both directions

### Immediate Rollback (First 48 Hours)

**Trigger**: Data loss, >1% error rate, performance <5k msgs/sec

```
Action:
  1. Route new traffic back to .NET service
  2. Keep Rust service running for diagnostics
  3. Analyze logs from Rust instance
  4. File issues and patches
  5. Redeploy Rust after fixes verified
  
Rollback time: <5 minutes (switch load balancer)
Data loss: 0 (Rust buffers remain intact)
```

### Phased Rollback

**Scenario**: Specific customer/location has issues

```
Action:
  1. Route that customer's traffic back to .NET
  2. Keep other traffic on Rust
  3. Investigate issue in isolation
  4. Fix and re-route customer
  
Enables: Per-customer migration if needed
Timeline: Weeks (not hours)
```

### Full Rollback (Week 1-4)

**Decision**: Rust implementation has unforeseen blockers

```
Action:
  1. Route all traffic back to .NET
  2. Archive Rust instance for analysis
  3. Schedule post-mortem review
  4. Plan fixes and re-migration
  
Contingency: Keep .NET running indefinitely
Duration: 1-2 weeks analysis, 1 month until re-attempt
```

---

## Success Metrics

### Performance Metrics

```
Target              .NET Baseline   Rust Target   Acceptance Criteria
──────────────────────────────────────────────────────────────────
Throughput          1-10k msgs/sec  10k+ msgs/sec  ≥10k msgs/sec
Latency (p99)       1-5ms           <2ms           ≤2ms
Memory baseline     50-100MB        <100MB         ≤100MB
CPU under load      5-10%           <5%            ≤5%
```

### Reliability Metrics

```
Target                          Goal        Acceptance
────────────────────────────────────────────────────────
Error rate                      <0.1%       <0.5%
Data loss                       0%          0%
Mean time between failure       >30 days    >90 days (target)
Mean time to recovery           <5 min      <10 min (max)
Log delivery success rate       >99.9%      >99.9%
```

### Testing Metrics

```
Target                          Goal        Acceptance
────────────────────────────────────────────────────────
Code coverage                   >90%        >85%
Integration test coverage       >85%        >80%
Load test sustained duration    72 hours    24 hours (min)
Spike test peak throughput      20k msgs/sec 15k (min)
```

---

## Post-Launch Activities

### Monitoring

```
Week 1: Hourly check-ins
Week 2-4: Daily reviews
Month 2-3: Weekly reviews
Month 4+: Quarterly reviews

Metrics tracked:
  - Throughput vs target
  - Latency (p50, p95, p99)
  - Error rates
  - Memory growth over time
  - Export success rate
  - Collector response times
```

### Optimization (If Needed)

```
If throughput <8k msgs/sec:
  - Profile (flamegraph)
  - Identify hotspots
  - Optimize allocation patterns
  - Consider batching strategy

If latency >5ms p99:
  - Check export backoff
  - Monitor collector performance
  - Reduce batch size if needed
  - Profile async task spawning

If memory growth:
  - Check for leaks (valgrind)
  - Monitor channel sizes
  - Verify buffering limits
  - Review batch sizes
```

### Decommissioning .NET Service

```
Timeline:
  - Week 1-4: Both running
  - Month 2: .NET in standby only
  - Month 3: .NET shutdown and archived
  - Quarter 2: .NET VM decommissioned (if on-prem)

Saves:
  - .NET runtime licensing (if applicable)
  - Windows server resources
  - Support/maintenance overhead
  - Migration tool development
```

---

## Communication Plan

### Stakeholders

- **PLC Engineers**: Need ADS protocol compatibility
- **IT Operations**: Need Windows service integration
- **Observability Team**: Need OTEL collector setup
- **Management**: Need timeline and risk assessment

### Milestones to Communicate

```
Phase 1 Complete (Day 30):
  "Core Rust service ready for testing"

Phase 2 Complete (Day 60):
  "Feature parity achieved, performance validated"

Phase 3 Complete (Day 90):
  "Ready for production deployment"

Production Go-Live (Day 95):
  "Migrating customers to Rust service"

Post-Launch Week 1 (Day 102):
  "Rust service stable, no critical issues"
```

---

**Document Status**: Complete  
**Review Frequency**: Weekly during phases, monthly post-launch  
**Last Updated**: March 31, 2026

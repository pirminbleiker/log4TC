# Log4TC Project Structure and Architecture Analysis

## Executive Summary

Log4TC is a Windows-based logging service for TwinCAT 3 PLCs by Beckhoff. It consists of two main components:
1. **PLC Library** - TwinCAT library for structured logging from the PLC
2. **.NET Service** - Windows service that receives logs via ADS and dispatches them through a plugin system

The user's goal is to:
- Replace all .NET code with native Rust
- Change the protocol to OpenTelemetry (OTEL) as the only API
- Keep and review the TwinCAT library for performance

---

## 1. Directory Structure (Full)

### Root Directory
```
/d/Projects/Open Source/log4TC/
├── .azure-pipelines/              # CI/CD configuration
├── .github/                        # GitHub workflows
├── docs/                           # Documentation
├── library/                        # TwinCAT compiled libraries
├── source/                         # .NET source code
├── azure-pipelines-ci.yml          # CI pipeline
├── azure-pipelines-release.yml     # Release pipeline
├── CONTRIBUTING.md
├── LICENSE                         # License file
└── readme.md
```

### Source Code Structure
```
source/Log4Tc/
├── Core Components:
│   ├── Log4Tc.Model/               # Data models (LogEntry, LogLevel)
│   ├── Log4Tc.Receiver/            # ADS receiver (receives logs from TwinCAT)
│   ├── Log4Tc.Dispatcher/          # Log routing/dispatch logic
│   └── Log4Tc.Service/             # Windows service entry point
│
├── Output Plugins:
│   ├── Log4Tc.Output/              # Base plugin interface
│   ├── Log4Tc.Output.NLog/         # NLog output plugin
│   ├── Log4Tc.Output.Graylog/      # Graylog output plugin
│   ├── Log4Tc.Output.InfluxDb/     # InfluxDB output plugin
│   └── Log4Tc.Output.Sql/          # SQL database output plugin
│
├── Infrastructure:
│   ├── Log4Tc.Plugin/              # Plugin framework and loader
│   ├── Log4Tc.Utils/               # Utility functions
│   └── Log4Tc.Setup/               # WiX installer project
│
├── Testing:
│   ├── Log4Tc.Model.Test/          # Data model tests (xUnit)
│   ├── Log4Tc.Dispatcher.Test/     # Dispatcher tests (xUnit)
│   ├── Log4Tc.Output.NLog.Test/    # NLog plugin tests
│   ├── Log4Tc.Output.Graylog.Test/ # Graylog plugin tests
│   ├── Log4Tc.Output.InfluxDb.Test/# InfluxDB plugin tests
│   ├── Log4Tc.SmokeTest/           # Integration tests
│   └── Mbc.Log4TcDispatcher.Test/  # Legacy test project
│
├── Other:
│   ├── Log4TcPrototype/            # Experimental/prototype code
│   ├── Log4Tc.sln                  # Main solution file
│   ├── Directory.Build.props        # Shared project properties
│   ├── build.cake                  # Cake build automation
│   ├── build.ps1                   # PowerShell build script
│   ├── build.sh                    # Bash build script
│   ├── NuGet.Config               # NuGet configuration
│   └── stylecop.json              # Code analysis rules
```

---

## 2. Solution and Project Files

### Main Solution
- **File**: `/d/Projects/Open Source/log4TC/source/Log4Tc/Log4Tc.sln`
- **Contains**: 21 projects (core + output plugins + tests + setup + docs)

### .NET Project Files Summary

| Project | Framework | Type | Purpose |
|---------|-----------|------|---------|
| **Log4Tc.Model** | netstandard2.0 | Library | Core data models (LogEntry, LogLevel) |
| **Log4Tc.Receiver** | netstandard2.0 | Library | ADS protocol receiver from TwinCAT |
| **Log4Tc.Dispatcher** | netstandard2.0 | Library | Log routing and dispatching logic |
| **Log4Tc.Service** | net6.0 | WinExe | Main Windows service application |
| **Log4Tc.Output** | netstandard2.0 | Library | Base output plugin interface |
| **Log4Tc.Output.NLog** | netstandard2.0 | Library | NLog sink plugin |
| **Log4Tc.Output.Graylog** | netstandard2.0 | Library | Graylog plugin |
| **Log4Tc.Output.InfluxDb** | netstandard2.0 | Library | InfluxDB plugin |
| **Log4Tc.Output.Sql** | (unknown) | Library | SQL database plugin |
| **Log4Tc.Plugin** | netstandard2.0 | Library | Plugin framework |
| **Log4Tc.Utils** | (unknown) | Library | Utility functions |
| **Log4Tc.Setup** | (WiX) | Setup | Windows installer |
| **Log4Tc.SmokeTest** | net6.0 | Test | Integration tests |
| **Test Projects** | net6.0 | Test | Unit tests (xUnit) |
| **Log4TcPrototype** | (unknown) | Experimental | Prototype/experimental code |
| **docs/docfx.csproj** | net6.0 | Documentation | DocFX documentation generator |

### TwinCAT Libraries
- **Location**: `/d/Projects/Open Source/log4TC/library/`
- **Files**:
  - `Log4TC.library` - Current TwinCAT library (compiled format)
  - `mbc_Log4TC.library` - Older version for reference

---

## 3. Architecture Overview

### System Architecture (High-Level)

```
┌─────────────────────────────────────────────────────┐
│                   TwinCAT Runtime (PLC)              │
│  ┌────────────────────────────────────────────────┐ │
│  │     Log4TC PLC Library (TwinCAT)                │ │
│  │  - Logger interface                             │ │
│  │  - Structured logging API                       │ │
│  │  - Message templates                            │ │
│  │  - Context properties                           │ │
│  └────────────────────────────────────────────────┘ │
│                        │                             │
│                        └─── ADS Protocol ────┐      │
└─────────────────────────────────────────────┼──────┘
                                              │
                    ┌─────────────────────────▼──────────────────┐
                    │    Log4TC Windows Service (.NET 6.0)       │
                    │                                             │
                    │  ┌─────────────────────────────────────┐   │
                    │  │  ADS Receiver                        │   │
                    │  │  - Listens on port 16150            │   │
                    │  │  - Decodes binary log messages      │   │
                    │  │  - Parses version 1 protocol        │   │
                    │  └─────────────────────────────────────┘   │
                    │                  │                          │
                    │                  ▼                          │
                    │  ┌─────────────────────────────────────┐   │
                    │  │  Log Dispatcher                      │   │
                    │  │  - Routes logs from receiver(s)     │   │
                    │  │  - Buffers using ActionBlock        │   │
                    │  │  - Dispatches to output plugins     │   │
                    │  └─────────────────────────────────────┘   │
                    │                  │                          │
                    │  ┌───┬───────┬───┴───┬──────────┐          │
                    │  │   │       │       │          │          │
                    │  ▼   ▼       ▼       ▼          ▼          │
                    │ NLog Graylog InfluxDB SQL    (Others)      │
                    │                                             │
                    └─────────────────────────────────────────────┘
```

### Component Interaction Flow

1. **TwinCAT PLC** creates structured log messages using Log4TC library
2. **ADS Protocol** transmits binary-encoded messages to port 16150 on the service
3. **ADS Receiver** (Log4Tc.Receiver) listens and decodes messages
4. **Log Entry** is created with metadata, arguments, and context
5. **Log Dispatcher** (Log4Tc.Dispatcher) receives LogEntry objects
6. **Output Plugins** (NLog, Graylog, InfluxDB, SQL) process and output the logs

---

## 4. Communication Protocol: ADS (Current)

### ADS Receiver Implementation
**File**: `/d/Projects/Open Source/log4TC/source/Log4Tc/Log4Tc.Receiver/AdsLogReceiver.cs`

### Protocol Details

**Current Implementation**:
- Protocol: **ADS (Automation Device Specification)** by Beckhoff
- Port: **16150**
- Server Name: "Log4Tc"
- Uses: `Beckhoff.TwinCAT.Ads` NuGet package (v6.1.298)

**Binary Message Format (Version 1)**:
```
[Version Byte]
[Message String] - Log message template
[Logger String]  - Logger name
[LogLevel]       - Log severity level
[PlcTimestamp]   - Timestamp from PLC (FILETIME)
[ClockTimestamp] - System clock timestamp (FILETIME)
[TaskIndex]      - Task index
[TaskName]       - Task name string
[TaskCycleCounter] - Cycle counter
[AppName]        - Application name
[ProjectName]    - Project name
[OnlineChangeCount] - Online change count

[Arguments Section]:
  While not end:
    [Type Byte = 1]:
      [Arg Index] [Arg Value] - Indexed argument
    [Type Byte = 2]:
      [Scope] [Name] [Value] - Context property
```

### LogEntry Data Model
**File**: `/d/Projects/Open Source/log4TC/source/Log4Tc/Log4Tc.Model/LogEntry.cs`

```csharp
public class LogEntry
{
    public string Source { get; set; }           // AMS source address
    public string Hostname { get; set; }         // PLC hostname
    public string Message { get; set; }          // Log message template
    public string Logger { get; set; }           // Logger name
    public LogLevel Level { get; set; }          // Log level
    public DateTime PlcTimestamp { get; set; }   // PLC-side timestamp
    public DateTime ClockTimestamp { get; set; } // System timestamp
    public int TaskIndex { get; set; }           // Task ID
    public string TaskName { get; set; }         // Task name
    public uint TaskCycleCounter { get; set; }   // Cycle counter
    public string AppName { get; set; }          // Application name
    public string ProjectName { get; set; }      // Project name
    public uint OnlineChangeCount { get; set; }  // Online change count
    
    public IDictionary<int, object> Arguments { get; }     // Message template args
    public IDictionary<string, object> Context { get; }    // Context properties
    public MessageFormatter MessageFormatter { get; }       // Message template parser
    public string FormattedMessage { get; }                 // Formatted message
}
```

### Message Template Support
- **Standard**: Message Templates Org (https://messagetemplates.org/)
- **Example**: `"Temperature is {temperature}°C at {timestamp}"`
- **Parser**: `MessageFormatter` class handles structured logging

---

## 5. Plugin System Architecture

### Plugin Framework
**Base File**: `/d/Projects/Open Source/log4TC/source/Log4Tc/Log4Tc.Output/ILogOutput.cs`

**Interface Pattern**:
- Plugins implement `ILogOutput` interface
- Plugins are discovered via `IPlugin` interface
- Plugin configuration via appsettings.json:
  ```json
  {
    "Outputs": [
      { "Type": "nlog" },
      { "Type": "graylog", "Host": "localhost", "Port": 12201 },
      { "Type": "influxdb", "Url": "http://localhost:8086" },
      { "Type": "sql", "ConnectionString": "..." }
    ]
  }
  ```

### Current Output Plugins

| Plugin | Implements | Exports To |
|--------|-----------|-----------|
| **NLog** | ILogOutput | File, Console, Database (via NLog) |
| **Graylog** | ILogOutput | Graylog GELF protocol |
| **InfluxDB** | ILogOutput | InfluxDB time-series database |
| **SQL** | ILogOutput | SQL Server, etc. |

### Plugin Loading
**File**: `/d/Projects/Open Source/log4TC/source/Log4Tc/Log4Tc.Plugin/PluginLoader.cs`

---

## 6. .NET Code Components (to be Replaced)

### Core .NET Components
| Component | Files | Key Purpose | Dependencies |
|-----------|-------|-------------|--------------|
| **Log4Tc.Service** | Program.cs | Windows service host | Microsoft.Extensions.Hosting |
| **Log4Tc.Receiver** | AdsLogReceiver.cs | ADS protocol listener | Beckhoff.TwinCAT.Ads |
| **Log4Tc.Dispatcher** | LogDispatcherService.cs | Log routing | System.Threading.Tasks.Dataflow |
| **Log4Tc.Output (base)** | - | Plugin interfaces | Microsoft.Extensions.* |
| **Output Plugins** | (NLog, Graylog, InfluxDB, SQL) | Format/send logs | Respective service SDKs |
| **Log4Tc.Plugin** | PluginLoader.cs | Plugin discovery | Reflection API |

### Key .NET Dependencies
- **Microsoft.Extensions.Hosting** v3.1.3
- **Microsoft.Extensions.Configuration** v3.1.3
- **Microsoft.Extensions.DependencyInjection** v3.1.3
- **Beckhoff.TwinCAT.Ads** v6.1.298 (for ADS protocol)
- **NLog** v4.7.0
- **Serilog** (logging infrastructure)
- **System.Threading.Tasks.Dataflow** v4.11.0

---

## 7. TwinCAT Library Assessment

### Current Library
- **Location**: `/d/Projects/Open Source/log4TC/library/Log4TC.library`
- **Format**: Compiled TwinCAT library (ZIP-based)
- **Status**: Actively maintained

### Library Capabilities
Based on documentation, the PLC library provides:
- Simple logging API for PLC programs
- **Structured logging** with message templates
- **Context properties** at various scopes (global, task, etc.)
- Argument binding for message templates
- Task metadata capture (index, name, cycle counter)
- Timestamp support (PLC time + system time)

### Performance Considerations
- Data is transferred from real-time task context to non-real-time via ADS
- ADS has bandwidth limitations
- Binary protocol is compact to minimize transfer overhead
- Currently uses version 1 of the binary protocol

---

## 8. Testing Infrastructure

### Test Projects
| Project | Framework | Type | Purpose |
|---------|-----------|------|---------|
| **Log4Tc.Model.Test** | net6.0 | xUnit | Data model and message template tests |
| **Log4Tc.Dispatcher.Test** | net6.0 | xUnit | Log routing and dispatch tests |
| **Log4Tc.Output.NLog.Test** | net6.0 | xUnit | NLog plugin tests |
| **Log4Tc.Output.Graylog.Test** | net6.0 | xUnit | Graylog plugin tests |
| **Log4Tc.Output.InfluxDb.Test** | net6.0 | xUnit | InfluxDB plugin tests |
| **Log4Tc.SmokeTest** | net6.0 | xUnit | Integration/smoke tests |

### Test Framework
- **Framework**: xUnit
- **Assertion Library**: FluentAssertions v5.10.3
- **Coverage**: coverlet.collector
- **Runner**: Visual Studio test explorer

### Build System
- **Primary**: Cake build automation (`build.cake`)
- **Scripts**: PowerShell (`build.ps1`) and Bash (`build.sh`)
- **Targets**: Clean, Build, Test, SmokeTest, Package, Publish

---

## 9. CI/CD Configuration

### CI/CD Files
- **Location**: `/d/Projects/Open Source/log4TC/.azure-pipelines/`
- **Pipelines**: 
  - `azure-pipelines-ci.yml` - Pull request/branch builds
  - `azure-pipelines-release.yml` - Tagged releases

### CI/CD Pipeline Flow

**CI Pipeline** (on PR to master/main/dev):
1. Build stage (Windows)
   - Restore dependencies
   - Build solution
   - Run unit tests
   - Generate test reports

**Release Pipeline** (on tag v*.*.*):
1. Build stage
2. Publish Release (GitHub releases with MSI artifacts)
3. Publish GitHub Pages (documentation)

### Build Tools
- **OS**: Windows 2019
- **Build Tool**: MSBuild (VS2019)
- **Frameworks**: .NET 6.0
- **Installer**: WiX Toolset (for MSI package)

---

## 10. Configuration Files

### Application Settings
**File**: `/d/Projects/Open Source/log4TC/source/Log4Tc/Log4Tc.Service/appsettings.json`

```json
{
  "Logging": {
    "LogLevel": {
      "Default": "Information",
      "Microsoft": "Warning"
    }
  },
  "Outputs": [
    { "Type": "nlog" },
    { "Type": "graylog", ... },
    { "Type": "influxdb", ... }
  ]
}
```

### Sample Configurations
- `appsettings.Development.json` - Development settings
- `appsettings.Production.json` - Production settings
- `appsettingsSample.json` - Example configuration
- `appsettingsInflux.json` - InfluxDB example
- `appsettingsSql.json` - SQL example

---

## 11. Documentation

### Key Documentation Files
- `/d/Projects/Open Source/log4TC/docs/internal/description.md` - System architecture (German)
- `/d/Projects/Open Source/log4TC/docs/internal/intro.md` - Introduction
- `/d/Projects/Open Source/log4TC/docs/gettingstarted/` - Getting started guides
- `/d/Projects/Open Source/log4TC/docs/reference/` - API reference

### Documentation Generation
- **Tool**: DocFX
- **Project**: `/d/Projects/Open Source/log4TC/docs/docfx.csproj`
- **Deployment**: GitHub Pages (`gh-pages` branch)

---

## 12. Key Findings and Recommendations

### For Rust Replacement

**Protocol Replacement Strategy**:
1. **Replace ADS with OpenTelemetry (OTEL)**:
   - OTEL provides standardized telemetry protocol
   - Supports both gRPC and HTTP transport
   - Better interoperability with cloud platforms
   - Consider OTLP (OpenTelemetry Line Protocol)

2. **Receiver Component**:
   - Current: `Log4Tc.Receiver` (ADS-based)
   - New: Rust HTTP/gRPC listener for OTEL
   - Must maintain compatibility with LogEntry model

3. **Dispatcher Component**:
   - Current: `LogDispatcherService` (async/await with ActionBlock)
   - New: Rust async runtime (tokio recommended)
   - Same buffering and routing logic

4. **Output Plugins**:
   - Framework must support similar plugin architecture
   - Consider trait-based design in Rust
   - Implement adapters for NLog, Graylog, InfluxDB, SQL

5. **Configuration**:
   - Current: JSON-based appsettings
   - Recommendation: Keep JSON format for compatibility
   - Use serde/toml for parsing

### TwinCAT Library Review Points

**Keep**: The TwinCAT library is well-designed and efficient
- Structured logging support via message templates
- Compact binary protocol (version 1) minimizes transfer
- Task metadata capture is valuable

**Recommend**:
- Review binary protocol v1 for OTEL compatibility
- Assess if direct OTEL instrumentation is better (eliminates .NET service)
- Consider direct TwinCAT->OTEL collector communication

### Service Installation

**Current**: Windows Service installation via WiX Toolkit
**Recommendation**: 
- Rust can also run as Windows Service (via windows-rs crate)
- Consider cross-platform service runners (systemd on Linux)
- Simplify installer when moving to Rust

---

## 13. File Path Reference

### Core Source Files
- **Receiver**: `/d/Projects/Open Source/log4TC/source/Log4Tc/Log4Tc.Receiver/`
  - `AdsLogReceiver.cs` - ADS server implementation
  - `ILogReceiver.cs` - Interface definition
  
- **Dispatcher**: `/d/Projects/Open Source/log4TC/source/Log4Tc/Log4Tc.Dispatcher/`
  - `LogDispatcherService.cs` - Routing logic
  - `OutputDispatch.cs` - Output management
  
- **Model**: `/d/Projects/Open Source/log4TC/source/Log4Tc/Log4Tc.Model/`
  - `LogEntry.cs` - Core data model
  - `LogLevel.cs` - Severity levels
  
- **Service**: `/d/Projects/Open Source/log4TC/source/Log4Tc/Log4Tc.Service/`
  - `Program.cs` - Service entry point
  - `appsettings.json` - Configuration
  
- **Output Plugins**: `/d/Projects/Open Source/log4TC/source/Log4Tc/Log4Tc.Output*/`
  - Various plugin implementations

- **TwinCAT Library**: `/d/Projects/Open Source/log4TC/library/`
  - `Log4TC.library` - Compiled library

### Build and Configuration
- **Solution**: `/d/Projects/Open Source/log4TC/source/Log4Tc/Log4Tc.sln`
- **Build Script**: `/d/Projects/Open Source/log4TC/source/Log4Tc/build.cake`
- **CI/CD**: `/d/Projects/Open Source/log4TC/azure-pipelines-*.yml`
- **Documentation**: `/d/Projects/Open Source/log4TC/docs/`

---

## Summary Table

| Aspect | Current Technology | Replacement Target |
|--------|-------------------|-------------------|
| Protocol | ADS (Beckhoff proprietary) | OpenTelemetry (OTEL) |
| Service Code | .NET 6.0 (C#) | Rust |
| Plugin System | .NET interfaces | Rust traits |
| Configuration | JSON (appsettings) | JSON (serde) |
| Testing | xUnit | Rust test framework (cargo test) |
| Build System | MSBuild/Cake | Cargo |
| PLC Library | TwinCAT (Keep) | TwinCAT (Keep) |
| Installer | WiX Toolkit | Windows installer Rust crate |
| CI/CD | Azure Pipelines | Azure Pipelines (same) |


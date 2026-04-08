# TwinCAT Library Optimization Plan

**Status**: GEPLANT - Umsetzung steht noch aus  
**Basiert auf**: `docs/twincat-performance-review.md`  
**Letzte Aktualisierung**: 2026-04-01

---

## Phase 1: API-Vereinfachung

### API-1: F_Log als einziger Entry Point

**Ziel**: Alle redundanten API-Funktionen entfernen. `F_Log` + Builder-Pattern deckt alle Use Cases ab.

**Entfernen** (30+ Funktionen/FBs):
- `F_LogL`, `F_LogC`, `F_LogLC`
- `F_LogA1` .. `F_LogA10`
- `F_LogLA1` .. `F_LogLA10`
- `F_LogLA1C` .. `F_LogLA10C`
- `F_LogL1` .. `F_LogL10`
- `F_LogTArg` (intern)
- `F_LogA` (intern)
- `FB_Logger`
- `FB_LoggerLAC`

**Behalten**:
- `F_Log()` — Einziger Entry Point fuer einfaches Logging
- `FB_Log` — **NEU**, Ersatz fuer FB_LoggerLAC (persistenter Logger + Context)
- `I_LogBuilder` — Fluent API Interface (bereinigt)
- `FB_LogBuilder` — Implementierung, **INTERNAL**
- `F_Context()` — Context Factory
- `FB_ScopedContext` — Scoped Context
- `FB_ContextBuilder` — bleibt INTERNAL

---

### API-2: FB_Log als Ersatz fuer FB_LoggerLAC

**Problem**: `FB_LoggerLAC` hat 10 feste ANY-Inputs, ist unflexibel und dupliziert Serialisierungs-Logik.

**Neuer FB_Log** — schlanker Wrapper um F_Log mit persistentem Logger + Context:

```
FUNCTION_BLOCK FB_Log
VAR
    sLogger         : T_MaxString;
    fbLoggerContext : FB_ContextBuilder;
END_VAR
```

**Methoden**:
- `Log(eLogLevel, sMessage) : I_LogBuilder` — gibt Builder zurueck mit vorgesetztem Logger + Context
- `LoggerContext : I_ContextBuilder` — Property fuer persistenten Context

**Verwendung**:
```
VAR
    fbLog : FB_Log(Const.sLoggerFromInstance);  // oder 'MyLogger'
END_VAR

// Logging mit persistentem Logger
fbLog.Log(eInfo, 'Motor {0} started')
    .WithAnyArg(nMotorId)
    .CreateLog();

// Persistenter Context (einmal setzen, immer dabei)
fbLog.LoggerContext.AddDInt('MachineId', nMachineId);
```

**Vorteile gegenueber FB_LoggerLAC**:
- Selbes Builder-Pattern wie F_Log (konsistent)
- Keine 10 festen ANY-Inputs
- Flexible Argumentanzahl
- Kein Reset-Boilerplate nach jedem Aufruf

---

### API-3: I_LogBuilder Interface bereinigen

**Aenderungen am Interface**:

1. `Clear()` **entfernen** — ist intern, wird nur von F_Log/FB_Log aufgerufen
2. `FB_LogBuilder` als **INTERNAL** markieren
3. `WithLogger` Parameter: `sLogger : REFERENCE TO T_MaxString` (spart 256B Kopie)
4. Methoden-Beschreibungen hinzufuegen

**Neues I_LogBuilder Interface**:
```
INTERFACE I_LogBuilder

// Adds a log argument. Supports all IEC 61131-3 types (BOOL, INT, REAL, STRING, etc.).
// Arguments are referenced by position in the message template: '{0}', '{1}', ...
// Values are serialized immediately — safe to call in loops with changing variables.
METHOD WithAnyArg : I_LogBuilder
VAR_INPUT
    stArg : ANY;
END_VAR

// Adds a pre-typed argument (T_Arg from Tc2_Utilities).
// Use WithAnyArg for most cases. WithTArg is for compatibility with Tc2 formatting.
METHOD WithTArg : I_LogBuilder
VAR_INPUT
    stArg : T_Arg;
END_VAR

// Overrides the default logger name for this log entry.
// If not called, the global logger name is used (or the FB_Log instance logger).
METHOD WithLogger : I_LogBuilder
VAR_INPUT
    sLogger : REFERENCE TO T_MaxString;
END_VAR

// Attaches context properties to this log entry.
// Use F_Context() to create a context builder, or pass an FB_ContextBuilder instance.
METHOD WithContext : I_LogBuilder
VAR_INPUT
    iContextBuilder : I_ContextBuilder;
END_VAR

// Finalizes and sends the log entry. Must be called to emit the log.
// After CreateLog(), the builder is consumed — do not reuse without a new F_Log() call.
METHOD CreateLog
```

---

## Phase 2: Performance-Optimierungen (bestehend, angepasst)

### OPT-1: Cache String Lengths (Effort: Low)

**Problem**: Wiederholte O(n) `LEN()` Aufrufe bei jeder Log-Operation  
**Dateien**: `FB_LogEntry.TcPOU`, `FB_LogBuilder.TcPOU`

**Aenderung**:
- In `_WriteString()`: Ergebnis von `LEN()` in lokaler Variable cachen
- Laenge einmal berechnen, mehrfach verwenden

**Erwartete Verbesserung**: ~10-15% CPU-Reduktion bei High-Frequency Logging

---

### OPT-2: sMessage als REFERENCE TO (Effort: Low)

**Problem**: `sMessage` (256 Bytes) wird 2x kopiert — einmal auf den Stack von F_Log, einmal in FB_LogBuilder.Clear()

**Aenderung**:
- `F_Log` Signatur: `sMessage : REFERENCE TO T_MaxString`
- `FB_LogBuilder.Clear()`: nur Pointer speichern, in `CreateLog()` serialisieren
- Pointer ist sicher: Caller haelt die Variable mindestens bis CreateLog() zurueckkehrt

**Erwartete Verbesserung**: 512 Bytes weniger Kopie pro Log-Aufruf

---

### OPT-3: Early Bounds Checking (Effort: Low)

**Problem**: Buffer-Overflow wird erst erkannt wenn Schreibversuch fehlschlaegt — Silent Data Loss  
**Dateien**: `FB_LogBuffer.TcPOU`, `FB_LogEntry.TcPOU`

**Aenderung**:
- `FB_LogEntry.Start()`: Minimale Message-Groesse pruefen BEVOR geschrieben wird
- `FB_LogBuffer`: Property `BufferFree` vor dem Schreiben pruefen
- Bei zu wenig Platz: `bOverflow := TRUE` setzen und Message skippen statt partiell schreiben

**Erwartete Verbesserung**: Verhindert korrupte/partielle Nachrichten, bessere Diagnostik

---

### OPT-4: Overflow Counter & Notification (Effort: Low)

**Problem**: Overflow passiert still, kein Feedback an Applikation  
**Dateien**: `FB_LogBuffer.TcPOU`, `FB_Log4TcTask.TcPOU`

**Aenderung**:
- `FB_LogBuffer`: `nOverflowCount : UDINT` Property hinzufuegen
- `FB_Log4TcTask`: `nDroppedMessages : UDINT` Property hinzufuegen
- Optional: Bei Overflow eine interne Log-Nachricht senden
- `Config.TcGVL`: `bLogOverflows : BOOL := TRUE;`

**Erwartete Verbesserung**: Sichtbarkeit von Datenverlust, besseres Monitoring

---

### OPT-5: Configurable ADS Retry (Effort: Low)

**Problem**: Hardcoded 5s Timeout, single retry, dann Discard  
**Dateien**: `FB_Log4TcTask.TcPOU`, `Config.TcGVL`

**Aenderung**:
- Neue Config-Variablen in `Config.TcGVL`:
  - `nAdsRetryCount : UINT := 1;`
  - `nAdsTimeoutMs : UDINT := 5000;`
  - `bDiscardOnFailure : BOOL := TRUE;`
- `FB_Log4TcTask`: State Machine um konfigurierbare Retries erweitern

**Erwartete Verbesserung**: Weniger Message Loss bei temporaeren ADS-Problemen

---

## Phase 3: Code-Bereinigung

### CLEAN-1: Duplikat-Code in FB_LogBuilder eliminieren

**Problem**: `WithAnyArg()` in FB_LogBuilder (~160 Zeilen) dupliziert `AddAnyArg()` in FB_LogEntry  
**Aenderung**: Beide nutzen denselben internen Serialisierungs-Pfad.
Da FB_LogBuilder einen eigenen Zwischen-Buffer (`fbLogBuffer`) benoetigt fuer die Reihenfolge-Unabhaengigkeit,
wird der CASE-Statement in eine gemeinsame Helper-Methode oder Helper-Function extrahiert.

---

### CLEAN-2: Dead Code entfernen

Nach API-Vereinfachung (Phase 1) entfernen:
- Alle F_LogA*, F_LogLA*, F_LogL* Funktionen
- FB_Logger, FB_LoggerLAC
- F_LogTArg, F_LogA (intern)
- Nicht mehr benoetigte Test-POUs aktualisieren

---

## Verworfene Optimierungen

| Idee | Grund fuer Verwerfung |
|---|---|
| Lazy Serialization (Args erst bei CreateLog) | Daten koennten sich zwischen WithAnyArg und CreateLog aendern. ANY-Pointer lebt auf Caller-Stack — nach Return ungueltig. |
| Double-Buffer eliminieren | Bricht Reihenfolge-Unabhaengigkeit der Fluent-Calls. WithLogger() nach WithAnyArg() waere nicht mehr moeglich. |
| Lookup-Table statt CASE fuer Typen | CASE wird vom Compiler als Jump-Table optimiert. Lookup-Table braucht FOR-Loop — im Worst-Case langsamer. Nur Wartungsgewinn. |
| Context Bulk-Copy (AddContextBlock) | Aendert Binary-Protokoll. Context-Overhead ist nur relevant bei vielen Properties — Empfehlung: Context sparsam verwenden statt Pfad optimieren. |
| CONCAT2 statt CONCAT | Nur 2 Stellen in F_InternalLog, selten aufgerufen. Kaum messbar. |

---

## Implementierungsreihenfolge

```
Phase 1 (API):        API-1, API-2, API-3, CLEAN-2    (~4-6h)
Phase 2 (Performance): OPT-1, OPT-2, OPT-3, OPT-4    (~3-4h)
Phase 3 (Cleanup):    CLEAN-1, OPT-5                   (~2-3h)
```

## Testplan

- Bestehende Tests auf neues API migrieren
- F_Log Builder-Pattern: alle Kombinationen testen (Args, Logger, Context)
- FB_Log: persistenter Logger + Context testen
- Binary Protocol Kompatibilitaet: Rust Parser muss optimierte Messages identisch parsen
- Performance Messung: Task Cycle Time mit/ohne Logging vergleichen

## Neues Public API (nach Umbau)

```
// Einfaches Logging
F_Log(eInfo, 'Motor {0} started')
    .WithAnyArg(nMotorId)
    .CreateLog();

// Mit Logger-Name
F_Log(eWarn, 'Temperature {0} too high')
    .WithLogger('Drives.Motor')
    .WithAnyArg(fTemp)
    .CreateLog();

// Mit Context
F_Log(eDebug, 'Cycle done')
    .WithContext(F_Context().AddDInt('BatchNr', nBatch))
    .CreateLog();

// Persistenter Logger (Ersatz FB_LoggerLAC)
VAR
    fbLog : FB_Log('Drives.Motor');
END_VAR
fbLog.LoggerContext.AddDInt('MachineId', 1);
fbLog.Log(eInfo, 'Started {0}').WithAnyArg(nId).CreateLog();

// Scoped Context
VAR
    fbScope : FB_ScopedContext;
END_VAR
fbScope.Begin(F_Context().AddString('Station', 'A1'));
F_Log(eInfo, 'Processing...').CreateLog();  // Context automatisch dabei
fbScope.End();
```

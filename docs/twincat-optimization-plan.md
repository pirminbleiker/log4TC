# TwinCAT Library Optimization Plan

**Status**: GEPLANT - Umsetzung steht noch aus  
**Basiert auf**: `docs/twincat-performance-review.md`  
**Constraint**: Keine Breaking Changes am Binary Protocol oder der Public API

---

## Optimierungen (6 Stück, priorisiert)

### OPT-1: Cache String Lengths (CRITICAL, Effort: Low)

**Problem**: Wiederholte O(n) `LEN()` Aufrufe bei jeder Log-Operation  
**Dateien**: `FB_LogEntry.TcPOU`, `F_LogTArg.TcPOU`, `F_LogA.TcPOU`

**Änderung**:
- In `FB_LogEntry._WriteString()`: Ergebnis von `LEN()` in lokaler Variable cachen
- In `FB_ContextBuilder._AddContext()`: String-Länge als Parameter akzeptieren (optional)
- Alle `F_LogA*` und `F_LogLA*` Funktionen: Länge einmal berechnen, mehrfach verwenden

**Erwartete Verbesserung**: ~10-15% CPU-Reduktion bei High-Frequency Logging

---

### OPT-2: Combine Small Value Writes / MEMCPY Reduktion (CRITICAL, Effort: Medium)

**Problem**: 15-25 einzelne MEMCPY-Aufrufe pro Nachricht, viele für 1-4 Bytes  
**Dateien**: `FB_LogEntry.TcPOU`

**Änderung**:
- Scratch-Buffer (16 Bytes) einführen für aufeinanderfolgende kleine Writes
- `_WriteByte`, `_WriteInt`, `_WriteDInt` sammeln in Scratch-Buffer
- Flush zu MEMCPY wenn Scratch voll oder String/Ende kommt
- Header-Block (Version + Level + Timestamps + TaskIndex) als ein MEMCPY

**Erwartete Verbesserung**: ~20-30% weniger MEMCPY-Aufrufe

---

### OPT-3: Early Bounds Checking (CRITICAL, Effort: Low)

**Problem**: Buffer-Overflow wird erst erkannt wenn Schreibversuch fehlschlägt - Silent Data Loss  
**Dateien**: `FB_LogBuffer.TcPOU`, `FB_LogEntry.TcPOU`

**Änderung**:
- `FB_LogEntry.Start()`: Minimale Message-Grösse prüfen (Header + Strings) BEVOR geschrieben wird
- `FB_LogBuffer`: Property `BufferFree` bereits vor dem Schreiben prüfen
- Bei zu wenig Platz: `bOverflow := TRUE` setzen und Message skippen statt partiell schreiben

**Erwartete Verbesserung**: Verhindert korrupte/partielle Nachrichten, bessere Diagnostik

---

### OPT-4: Configurable ADS Retry (HIGH, Effort: Low)

**Problem**: Hardcoded 5s Timeout, single retry, dann Discard  
**Dateien**: `FB_Log4TcTask.TcPOU`, `Config.TcGVL`

**Änderung**:
- Neue Config-Variablen in `Config.TcGVL`:
  - `nAdsRetryCount : UINT := 1;` (Anzahl Retries)
  - `nAdsTimeoutMs : UDINT := 5000;` (Timeout in ms)
  - `bDiscardOnFailure : BOOL := TRUE;` (Buffer verwerfen bei Failure)
- `FB_Log4TcTask`: State Machine um konfigurierbare Retries erweitern

**Erwartete Verbesserung**: Weniger Message Loss bei temporären ADS-Problemen

---

### OPT-5: Overflow Counter & Notification (HIGH, Effort: Low)

**Problem**: Overflow passiert still, kein Feedback an Applikation  
**Dateien**: `FB_LogBuffer.TcPOU`, `FB_Log4TcTask.TcPOU`

**Änderung**:
- `FB_LogBuffer`: `nOverflowCount : UDINT` Property hinzufügen
- `FB_Log4TcTask`: `nDroppedMessages : UDINT` Property hinzufügen
- Optional: Bei Overflow eine interne Log-Nachricht senden (wenn Platz im nächsten Buffer)
- `Config.TcGVL`: `bLogOverflows : BOOL := TRUE;`

**Erwartete Verbesserung**: Sichtbarkeit von Datenverlust, besseres Monitoring

---

### OPT-6: Cache Pointer Offsets in Context Builder (MEDIUM, Effort: Low)

**Problem**: O(n) lineare Suche in Property-Arrays bei jedem Add/Find/Remove  
**Dateien**: `FB_ContextBuilder.TcPOU`

**Änderung**:
- Hash-basierter Index für Property-Namen (einfacher Modulo-Hash auf ersten 2 Chars)
- Oder: Sortierte Einfügung + Binary Search statt linearer Suche
- Alternativ (einfacher): Name-zu-Index Mapping als festes Array

**Erwartete Verbesserung**: ~5-10% bei vielen Context-Properties (>10)

---

## Implementierungsreihenfolge

```
Phase 1 (Quick Wins):     OPT-1, OPT-3, OPT-5  (~2-3h)
Phase 2 (Core Perf):      OPT-2, OPT-4          (~4-6h)  
Phase 3 (Nice-to-have):   OPT-6                  (~2h)
```

## Testplan

- Bestehende TwinCAT Tests müssen weiterhin bestehen (`FB_ContextBuilder_Test`, `PRG_Test`)
- Binary Protocol Kompatibilität: Rust Parser muss optimierte Messages identisch parsen
- Performance Messung: Task Cycle Time mit/ohne Logging vergleichen
- Stress Test: 100Hz Logging mit 4 Arguments + 4 Context Properties

## Risiken

- IEC 61131-3 hat keine generischen Buffer/Streaming APIs → MEMCPY-Optimierung limitiert
- Scratch-Buffer braucht zusätzlichen Speicher pro Task (~16 Bytes)
- Hash-basierter Index in OPT-6 könnte Kollisionen haben bei ähnlichen Namen

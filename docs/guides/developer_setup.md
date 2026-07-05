# Developer Setup & Architecture Onboarding Guide

Welcome to the **NOVA** AI Operating Platform workspace. This guide helps developers understand the repository layout, system design patterns, and get their local environment compiled and tested.

## 1. Repository Layout

NOVA's on-device-first codebase is structured as a Rust Cargo workspace containing a shared core microkernel, isolated modules, and a stable FFI boundary for platform shells.

```
NOVA/
├── api/
│   └── ffi/                  # [NEW] Stable C-ABI FFI boundary for platform shells (JNI / DLL)
├── config/
│   └── default.toml          # [NEW] Default private-first system configuration settings
├── docs/
│   ├── bible/                # Chapters 0, 1, 2 of the NOVA specification
│   ├── adr/                  # Architectural Decision Records (ADR-0001 to ADR-0010)
│   └── guides/
│       └── developer_setup.md # This document
├── modules/                  # Modular engines hosted by the microkernel
│   ├── memory/               # Memory Engine skeleton
│   ├── search/               # Universal Search skeleton
│   ├── voice/                # Voice assistant system skeleton
│   ├── ai/                   # AI inference Engine skeleton
│   ├── comms/                # Device Communication and synchronization
│   └── plugin_host/          # WASM and sandboxed plugin host
├── src/
│   └── kernel/               # [NEW] NOVA Microkernel (lifecycle, Event Bus, Config, Logging)
├── tests/                    # Integration, E2E, performance test folders
├── scripts/
│   └── build.ps1             # [NEW] PowerShell script for linting, compiling, and testing
└── Cargo.toml                # [NEW] Root Cargo workspace config
```

---

## 2. Kernel Core Substrates

The microkernel core contains the foundational patterns used throughout the codebase:

### A. Asynchronous Event Bus (`src/kernel/src/event_bus.rs`)
- **Pub/Sub Mode:** Multi-subscriber broadcast channel. Used for system-wide notices like memory capture.
- **Request/Response Mode:** Exclusive handler-registered message passing for direct commands (e.g. `search:query`).
- **Provenance Metadata:** Every event carries an ID, timestamp, calling origin module, and correlation ID to preserve transparency (Principle 5).

### B. Layered Configuration (`src/kernel/src/config.rs`)
- Configuration loads layered settings: secure defaults -> user local settings (`local.toml`) -> device tier overrides.
- Explicit schema-validation ensures privacy and autonomy dials always fail-safe by defaulting conservative.

### C. Logging & Redaction (`src/kernel/src/logger.rs`)
- Local-only diagnostic tracing. Redacts PII through the `Redacted<T>` wrapper.
- Structured user-facing logs: **Activity Trail** (transparency) and **Egress Log** (network isolation auditing).

### D. Structured Error Taxonomy (`src/kernel/src/error.rs`)
- Errors are classified into domain categories with stability codes and correlation IDs. No sensitive information is leaked in error texts.

---

## 3. Getting Started

### Prerequisites
- Install [Rust & Cargo](https://rustup.rs/) (Edition 2021).
- PowerShell (on Windows) or Bash.

### Verification Steps
Run the automation verification script to clean, format, lint, compile, and execute tests:

```powershell
# Windows
.\scripts\build.ps1
```

Or execute commands manually:
```bash
# Verify formatting
cargo fmt --all -- --check

# Run lints
cargo clippy --all-targets -- -D warnings

# Build all modules & dynamic library
cargo build --all-targets

# Run the test suite
cargo test --workspace
```

---

## 4. Guidelines for Adding Code
1. **No placeholders:** Write clean, complete implementation code where possible.
2. **Privacy First:** Never exfiltrate data. Every egress must route through the `Egress Gate`.
3. **No direct calls:** Modules must communicate *exclusively* via event-bus message passes or structured request/response pipelines.
4. **Redact PII:** Wrap PII strings in `logger::Redacted(...)` before logging diagnostics.

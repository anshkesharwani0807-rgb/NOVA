# ADR-0002 — Cross-Platform Strategy (Shared Core + Native Shells)

- **Decision ID:** ADR-0002
- **Status:** Proposed
- **Date:** 2026-07-04
- **Governed by:** D6 (Android+Windows first; Linux/macOS later), D1, Principle 3,
  LG-3 (platform breadth). **Builds on:** ADR-0001.

## Context

NOVA must feel native and perform well on Android and Windows now, and extend to
Linux/macOS later, without rewriting the intelligence, memory, and security logic per
platform. We must decide how to divide code between a shared core and platform shells.

## Options Considered

1. **Shared Rust core + platform-native UI shells** (Android: Kotlin/Jetpack Compose;
   Windows: native shell over the core). Maximum native feel; UI written per platform.
2. **Shared Rust core + single cross-platform UI toolkit** (e.g. Compose Multiplatform
   or Flutter over the core). One UI codebase; less native fidelity; extra dependency.
3. **Fully native per platform (no shared core).** Best native feel; unacceptable
   duplication of security/memory/AI logic — rejected (violates coherence, multiplies
   trust-critical surface).
4. **Fully cross-platform framework for everything (e.g. Flutter incl. logic).**
   Rejected: weak for heavy on-device compute/crypto (see ADR-0001).

## Chosen Solution

**Option 1 as the canonical path, with Option 2 (Compose Multiplatform) recorded as a
sanctioned fallback to reduce UI duplication if it proves too costly.**

- **Shared core (Rust):** all intelligence, memory, search, crypto, inference,
  event bus, plugin host, and the consent/egress gates. Platform-agnostic. One code
  path for trust-critical logic (reduces attack surface — Principle 2).
- **Android shell:** Kotlin + Jetpack Compose, binding to the core via JNI over the
  C-ABI. Runs the core in a background service for always-available capture.
- **Windows shell:** a native desktop shell binding to the core via the C-ABI. UI
  toolkit choice (WinUI/Win32 vs. a cross-platform desktop toolkit) is deferred to a
  UI ADR under Ch17, but the **core boundary is identical** to Android.
- **Later (Linux/macOS):** new shells over the *same* core; no core changes expected.

## Trade-offs

- **(-) UI is written twice** (Android + Windows). *Accepted* for native fidelity;
  *mitigated* by keeping as much presentation-agnostic logic as possible in the core
  (view-models/state can live near the core). Fallback: Compose Multiplatform.
- **(+) Trust-critical code exists once**, in a memory-safe language — the single most
  important security property (Principle 2).
- **(+) Clean seam for Linux/macOS** later (LG-3) with no core rework.

## Consequences

- Defines two build targets now (Android AAR/native lib; Windows native lib + shell)
  and a shared core crate. Reflected in ADR-0014 and the module layout (Ch6).
- The C-ABI seam becomes a first-class, versioned interface (see IPC spec, Ch12).
- The "one core, many shells" shape constrains ADR-0003 and ADR-0004.

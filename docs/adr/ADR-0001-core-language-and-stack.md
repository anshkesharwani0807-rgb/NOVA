# ADR-0001 — Core Language & Technology Stack

- **Decision ID:** ADR-0001
- **Status:** Proposed
- **Date:** 2026-07-04
- **Resolves:** OQ-2 (concrete stack). **Governed by:** D1 (on-device-first), D5
  (concrete recommendation), D6 (Android+Windows first), Principle 7 (longevity,
  no fatal vendor lock-in), Principle 3 (offline capability).

## Context

NOVA is on-device-first (D1) and must run a capable local core — memory store,
semantic search, cryptography, and AI inference — on **Android and Windows** now, and
**Linux/macOS** later (D6, LG-3). The core is compute- and memory-intensive, security-
critical (Principle 2), and must last for years across device generations (Principle
7). We need a primary language and stack for the **shared core**, separate from the
platform UI shells (see ADR-0002).

Selection criteria, weighted by the principles:

1. **Portability** to Android (NDK) and Windows natively, plus Linux/macOS later.
2. **Performance & control** for on-device ML, indexing, and crypto within tight
   battery/memory budgets (Principle 3, Ch16).
3. **Memory safety & security** for a product whose identity is trust (Principle 2).
4. **Longevity & independence** — no runtime/vendor whose disappearance is fatal (P7).
5. **FFI quality** to bind cleanly to native platform UIs and ML libraries.
6. **Ecosystem** for crypto, embedded storage, and inference.

## Options Considered

| Option | Portability | Perf/control | Safety | Longevity | FFI | Notes |
|---|---|---|---|---|---|---|
| **Rust (core) + native UI** | Excellent | Excellent | Excellent (borrow checker) | Excellent (no runtime, open) | Excellent (C ABI) | Strong crypto/embedded/ML crates; steep learning curve |
| **C++ (core) + native UI** | Excellent | Excellent | Poor (manual memory) | Good | Excellent | Max ecosystem but memory-unsafe — bad fit for trust product |
| **Kotlin Multiplatform** | Good (great Android; desktop weaker) | Good (JVM/native) | Good | Good (JetBrains-led) | Good | Excellent Android; heavy-native/Windows story weaker |
| **.NET / C# (MAUI)** | Good (great Windows; Android ok) | Good | Good (GC) | Good (MS-led) | Good | Windows-strong; some vendor gravity; GC pauses |
| **Flutter / Dart** | Good UI everywhere | Moderate (Dart weak for heavy compute) | Good | Google-led (P7 concern) | Moderate | Great UI, weak heavy-compute core |
| **Go (core)** | Good | Good | Good (GC) | Good | Moderate (cgo friction) | GC + mobile story weaker for tight budgets |

## Chosen Solution

**A shared core written in Rust, exposed over a stable C-ABI FFI, with platform-native
UI shells (detailed in ADR-0002).**

- **Core language: Rust.** It uniquely satisfies portability + performance + memory
  safety + longevity + FFI simultaneously. It has no heavy runtime (good for battery/
  memory), a strong safety story (critical for a trust product), mature crypto and
  embedded-storage crates, and compiles to Android (NDK) and Windows natively, with
  Linux/macOS effectively free later.
- **FFI boundary:** the core presents a narrow, stable C-ABI surface; UI shells and
  platform integrations bind to it. This keeps the core portable and the UIs native.

## Trade-offs

- **(-) Learning curve / hiring.** Rust is harder to learn than Kotlin/C#. *Mitigation:*
  a small, well-factored core minimizes the Rust surface; UI teams work in familiar
  native languages.
- **(-) Slower initial velocity** than a GC language. *Accepted:* safety and longevity
  outrank early speed for a multi-year trust product (Principles 2, 7).
- **(-) FFI friction** across the boundary. *Mitigation:* keep the boundary narrow and
  well-specified (ADR-0004, and the IPC spec).
- **(+) No fatal vendor dependency** (P7): Rust is open, community-governed, multi-
  backend (LLVM/GCC). No single company's exit kills NOVA.

## Consequences

- ADR-0002 (cross-platform), ADR-0003 (architecture), ADR-0005 (concurrency), and all
  Step-3 specs assume a Rust core with a C-ABI seam.
- Build/toolchain (ADR-0014) targets Rust + platform toolchains (Android NDK/Gradle,
  Windows MSVC).
- Hiring/onboarding plans (Ch18) must account for Rust expertise on the core team.
- This decision is reversible only at high cost; it is the foundational stack choice.

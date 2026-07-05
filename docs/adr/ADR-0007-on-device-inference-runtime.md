# ADR-0007 — On-Device AI Inference Runtime

- **Decision ID:** ADR-0007
- **Status:** Proposed
- **Date:** 2026-07-04
- **Governed by:** D1 (on-device-first), Principle 3 (offline), Principle 7 (no fatal
  vendor lock-in), the acceleration seam (Ch1), Ch11 (AI Engine), Ch16 (resources).

## Context

NOVA's baseline intelligence must run **locally** (D1): language understanding/
generation, embeddings for search, and speech (ASR/TTS). It must run within device
budgets (Ch16), remain useful offline (Principle 3), and — critically — must **not** bind
NOVA's survival to any single model vendor or runtime (Principle 7). An optional
**acceleration seam** may offload heavy work when the user consents and connectivity
exists, but it is never required.

## Options Considered

1. **Single vendor SDK / proprietary runtime.** Simple integration; violates Principle 7
   (fatal dependency) and often assumes cloud. Rejected as the primary path.
2. **Open, portable inference runtime(s) for quantized local models** (e.g. a
   GGUF/llama.cpp-class engine for LLMs; an ONNX-Runtime-class engine for embeddings/
   ASR/TTS and general models), abstracted behind a NOVA-owned interface.
3. **Write our own inference engine.** Enormous cost, no benefit over mature open
   engines. Rejected.

## Chosen Solution

**A NOVA-owned `InferenceRuntime` abstraction (interface) with pluggable open,
on-device backends (Option 2), plus an optional consent-gated remote backend behind the
same interface (the acceleration seam).**

- **Abstraction first (Principle 7):** the AI Engine depends on NOVA's own runtime
  interface, never directly on a specific engine. Backends are swappable; no single
  engine's disappearance is fatal.
- **Local backends:** a quantized-LLM backend (GGUF/llama.cpp-class) for language, and
  an ONNX-Runtime-class backend for embeddings, ASR, and TTS. Hardware acceleration
  (NPU/GPU where available) is used opportunistically per device tier (Ch16).
- **Model management:** models are local assets with recorded versions/licenses
  (dependency risk D-RISK-3); model files are git-ignored and never committed.
- **Acceleration seam:** a remote backend implementing the same interface MAY be used
  **only** when (a) the user consents and (b) the Egress Gate authorizes it (D3). It is
  an enhancement over a working local baseline, never a precondition (D1).

## Trade-offs

- **(-) Abstraction overhead** and the effort of maintaining multiple backends.
  *Accepted:* it is the price of Principle 7 independence and multi-device hardware
  reach.
- **(-) Local models lag frontier cloud** (R1). *Mitigated* by the consent-gated seam
  and improving local models (XG-1); honesty about limits (Principle 9).
- **(+) Offline-capable intelligence** — the core promise (Principle 3).
- **(+) No fatal vendor lock-in** — backends and models are replaceable.

## Consequences

- Ch11 (AI Engine) specifies the runtime interface, model lifecycle, routing (local vs.
  seam), and uncertainty surfacing (Principle 9).
- The Egress Gate (D3) is a hard dependency for the acceleration seam.
- Model storage/growth and thermal/battery cost feed Ch16; model licensing feeds the
  dependency policy (Ch18).

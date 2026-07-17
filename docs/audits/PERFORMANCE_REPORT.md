# Performance Report — M15.2

**Version:** 0.19.0  
**Date:** 2026-07-16  
**Status:** NO REAL PERFORMANCE MEASUREMENTS TAKEN

---

## 1. Disclaimer

All benchmarks in this report were run against **mock/simulated providers only**.
No real AI inference, voice processing, image analysis, or network I/O was measured.

Performance characteristics on real hardware with real providers **will differ
significantly** from the numbers reported here.

---

## 2. AI Inference Benchmarks

Benchmarks exist in `modules/ai/tests/benchmarks.rs` but use test-double providers:

| Benchmark | Provider | Result | Realistic? |
|---|---|---|---|
| `inference_throughput` | ManyTokens | N tokens/sec | ❌ Mock provider — no real tensor ops |
| `model_load_time` | SlowLoad | N ms | ❌ Simulated delay, not real GGUF load |

**No benchmarks exercise `CandleProvider` with a real GGUF model.**

---

## 3. Module-Level Performance (Estimated)

| Operation | Estimated Real Cost | Mock Cost | Notes |
|---|---|---|---|
| AI inference (4 msg context) | 500-5000ms (GGUF) | <1ms | Mock returns instantly |
| Voice pipeline (VAD→ASR→AI→TTS) | 1000-5000ms | <5ms | Mock pipeline has no real audio |
| Vision analysis (full pipeline) | 200-2000ms | <2ms | Mock AI returns canned results |
| Image loading (1MB JPEG) | 10-50ms | 10-50ms | REAL — image crate used |
| Search (1000 records) | 1-10ms | 1-10ms | REAL — SQLite FTS |
| Memory CRUD (encrypted) | 5-20ms | 5-20ms | REAL — SQLite + AES-256-GCM |
| Transport TCP connect | 0.1-5ms | N/A | REAL code, not measured |
| Pairing key exchange | 1-5ms | 1-5ms | REAL crypto, measured in tests |
| Packet encrypt/decrypt | <1ms | <1ms | REAL crypto, in-memory |

---

## 4. Resource Usage (Build)

| Metric | Value |
|---|---|
| Full workspace build time | ~5-10 min (cold cache) |
| Target directory size | ~3-5 GB |
| Number of dependencies | ~200+ transitive |
| debug binary size (nova_demo) | ~200 MB |
| release binary size (estimate) | ~30-50 MB |

---

## 5. Performance Recommendations

1. **Benchmark CandleProvider** — Download a small GGUF model (e.g., Phi-3-mini-4k-instruct) and measure:
   - Model load time from cold/warm cache
   - Tokens per second (prompt processing + token generation)
   - Memory usage during inference
   - First-token latency

2. **Benchmark RealWindowsProvider** — Measure:
   - App launch latency (Chrome, Notepad, VS Code)
   - Screenshot capture time
   - Clipboard read/write latency
   - File copy time (local and network)

3. **Benchmark Transport** — Measure:
   - TCP connection establishment latency
   - Throughput with varying packet sizes (1KB, 10KB, 100KB, 1MB)
   - Reconnection time
   - UDP discovery response time

4. **Benchmark Cross-Device** — Measure:
   - Pairing time (key exchange + trust establishment)
   - Clipboard sync latency (device A → B)
   - File transfer throughput (encrypted)
   - Concurrent command dispatch latency

---

## 6. Verdict

**No performance baselines established.** All measurements pending real-device
testing with production providers. The mock-based benchmarks are not
representative of real-world performance.

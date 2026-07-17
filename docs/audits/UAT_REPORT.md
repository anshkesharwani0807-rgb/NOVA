# UAT Report — M15.2 User Acceptance Testing

**Version:** 0.19.0  
**Date:** 2026-07-16  
**Status:** CODE-LEVEL VALIDATION COMPLETED — REAL-DEVICE UAT PENDING

---

## Important Notice

This UAT was performed **entirely at the code level** within the Rust workspace.
**No real devices (Android phone, Windows PC as remote agent) were used.**
All cross-device, voice, vision, and OS-interaction tests were conducted using
mock/simulated providers.

---

## 1. Android Validation — NOT TESTED ON REAL DEVICE

| Test Case | Status | Environment | Details |
|---|---|---|---|
| APK Build | ❌ PENDING | No buildozer/gradle in workspace | No Android build system present |
| APK Install | ❌ PENDING | No APK | — |
| Physical Phone | ❌ PENDING | No device connected | — |
| Emulator | ❌ PENDING | No emulator configured | — |
| Launch Success | ❌ PENDING | — | — |
| Cold Start | ❌ PENDING | — | — |
| Warm Start | ❌ PENDING | — | — |
| Background/Foreground | ❌ PENDING | — | — |
| Rotation | ❌ PENDING | — | — |
| Permission Flow | ❌ PENDING | — | — |
| Camera | ❌ PENDING | — | — |
| Gallery | ❌ PENDING | — | — |
| Offline Mode | ❌ PENDING | — | — |
| Battery | ❌ PENDING | — | — |

**What was validated:** `api/jni/` crate compiles successfully as cdylib.
16 JNI entry points defined. No runtime testing possible without Android runtime.

---

## 2. Windows Validation — NOT TESTED WITH REAL PROVIDER

| Test Case | Status | Environment | Details |
|---|---|---|---|
| Startup | ✅ CODE-ONLY | MockWindowsProvider | KernelModule start/stop tested in-memory |
| System Tray | ❌ PENDING | RealWindowsProvider never invoked | — |
| Clipboard | ✅ CODE-ONLY | MockWindowsProvider | Set/get clipboard in mock |
| File APIs | ❌ PENDING | RealWindowsProvider never invoked | — |
| Notifications | ❌ PENDING | RealWindowsProvider never invoked | — |
| Process Control | ✅ CODE-ONLY | MockWindowsProvider | Launch/close/kill in mock |
| Volume/Brightness | ❌ PENDING | RealWindowsProvider never invoked | — |
| Lock/Shutdown/Sleep | ❌ PENDING | RealWindowsProvider never invoked | — |
| Screenshot | ❌ PENDING | RealWindowsProvider never invoked | — |
| Shutdown/Restart | ❌ PENDING | RealWindowsProvider never invoked | — |
| Reconnect | ❌ PENDING | — | — |

**What was validated:** `RealWindowsProvider` code exists (659 lines) with
correct PowerShell/cmd command construction. 6 unit tests cover
`MockWindowsProvider` paths only.

---

## 3. Cross-Device Validation — SIMULATED ONLY

| Test Case | Status | Environment | Details |
|---|---|---|---|
| Android→Windows | ✅ SIMULATED | In-memory mock adapters | WindowsAdapter wraps MockWindowsProvider |
| Clipboard Sync | ✅ SIMULATED | In-memory SyncManager | Data stored in Vec, no network |
| File Transfer | ✅ SIMULATED | In-memory with real crypto | Real encryption, simulated transfer |
| Memory Sync | ✅ SIMULATED | In-memory sync store | No actual remote sync |
| Automation Sync | ❌ PENDING | No cross-device automation tested | — |
| Trusted Device | ✅ CODE-ONLY | In-memory TrustedDeviceStore | Real key exchange, simulated pairing protocol |
| Reconnect | ❌ PENDING | — | — |
| Device Removal | ✅ CODE-ONLY | In-memory store | Tested at unit level |
| Key Rotation | ✅ CODE-ONLY | Real security manager | Rotation tested at unit level |
| Permission Changes | ✅ CODE-ONLY | PermissionManager | Tested at unit level |
| Phone Hotspot | ❌ PENDING | No real network | — |
| Home Wi-Fi | ❌ PENDING | No real network | — |
| Offline | ✅ CODE-ONLY | Egress gate blocks | Tested at unit level |

**What was validated:** All cross-device orchestration logic tested in-process.
Discovery returns empty (no real discovery). Transport never started in demo.

---

## 4. Security Audit — CODE LEVEL ONLY

See `SECURITY_AUDIT.md` for full details.

| Attack Type | Status | Details |
|---|---|---|
| Unknown device | ✅ CODE-ONLY | Trusted device check verified |
| Replay attack | ❌ PENDING | No replay protection tests |
| Invalid signature | ✅ CODE-ONLY | ed25519 verification tested |
| Expired key | ✅ CODE-ONLY | Key rotation + grace period tested |
| Tampered packet | ❌ PENDING | No tampered-packet tests |
| Permission escalation | ✅ CODE-ONLY | PermissionManager deny-by-default tested |
| Plugin sandbox escape | ✅ CODE-ONLY | Sandbox action deny tested |
| Unauthorized file access | ❌ PENDING | No file access boundary tests |
| Unauthorized clipboard access | ❌ PENDING | No clipboard boundary tests |
| Unauthorized memory access | ✅ CODE-ONLY | Permission-gated memory read tested |

---

## 5. Performance — BENCHMARKS ON MOCK PROVIDERS ONLY

See `PERFORMANCE_REPORT.md` for full details.

| Metric | Status | Details |
|---|---|---|
| Cold start | ❌ NOT MEASURED | — |
| Warm start | ❌ NOT MEASURED | — |
| Memory usage | ❌ NOT MEASURED | — |
| CPU | ❌ NOT MEASURED | — |
| Disk | ❌ NOT MEASURED | — |
| Network latency | ❌ NOT MEASURED | — |
| Clipboard latency | ❌ NOT MEASURED | — |
| File transfer speed | ❌ NOT MEASURED | — |
| Search latency | ❌ NOT MEASURED | — |
| Memory retrieval | ❌ NOT MEASURED | — |
| Voice latency | ❌ NOT MEASURED | — |
| Vision latency | ❌ NOT MEASURED | — |
| Automation latency | ❌ NOT MEASURED | — |
| AI inference | ⚠️ MOCK ONLY | Benchmarks cover mock providers only |

---

## 6. Overall UAT Verdict

### Code-Level Validation: ✅ COMPLETE

All 23 crates compile, 4 CI gates pass, 1100+ unit tests pass, demo runs to
completion. Module APIs are consistent, event bus flows work, permissions are
enforced, crypto operations are verified, plugin lifecycle is correct.

### Real-Device / Production Validation: ❌ PENDING

| Area | What's Needed |
|---|---|
| **Android** | Set up buildozer/gradle, build APK, install on emulator or physical device, test all 15 UAT scenarios |
| **Windows** | Create a test harness that uses `RealWindowsProvider`, verify clipboard, files, notifications, process control, volume, brightness, screenshot on actual Windows |
| **Cross-Device** | Set up two devices (or two processes) on same network, test pairing, clipboard sync, file transfer, discovery |
| **Voice** | Connect real microphone and speaker, test VAD, wake word, ASR, TTS pipeline |
| **Vision** | Connect real AI models (or external API) for OCR, caption, detection, face analysis |
| **AI** | Download GGUF model, test CandleProvider inference end-to-end |
| **Performance** | Measure all metrics on real hardware |
| **Stress** | Run 1000 parallel operations, repeated pair/disconnect, reconnect storms |
| **Security** | Test replay attacks, tampered packets, unauthorized access on real transport layer |

---

## 7. Recommendation

**Do not declare M15.2 complete until at least one real-device validation
scenario is executed.** Minimum viable real-device evidence:

1. WindowsAgent with RealWindowsProvider: launch an app, get clipboard, set volume
2. Cross-device: pair two processes over TCP, sync clipboard text
3. Android: build and launch the JNI crate in an emulator

Without these, the audit remains a **code-review-level validation only**.

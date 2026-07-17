# Security Audit — NOVA v0.18.5-m15.2

**Generated:** 2026-07-16  
**Milestone:** M15.2  
**Auditor:** Automated + Manual Review

---

## 1. Threat Model Coverage

| Principle (BRAIN.md) | Implementation | Verified |
|---------------------|----------------|----------|
| 1. User Sovereign | ConsentManager grants user final say | ✅ |
| 2. Privacy by Default | `local_by_default=true`, telemetry off | ✅ |
| 3. On-Device First | All inference/storage local | ✅ |
| 4. Memory Sacred | Encrypted at rest (AES-256-GCM) | ✅ |
| 5. Transparency | Activity Trail + Egress Log | ✅ |
| 6. Agency with Consent | EgressGate blocks without consent | ✅ |
| 7. Longevity/Ownership | Open formats, no vendor lock-in | ✅ |
| 8. Coherence over Features | No feature creep | ✅ |
| 9. Honesty About Limits | Uncertainty surfacing in AI | ✅ |

---

## 2. Attack Surface Analysis

### 2.1 Network Egress (EgressGate)

| Policy | Blocks | Allows |
|--------|--------|--------|
| `OfflineOnly` | All network | None |
| `LocalNetworkOnly` | Internet | LAN/mDNS/loopback |
| `InternetAllowed` | None (but consent required) | Internet + LAN |
| `Blocked` | All | None |

**Tests passed:**
- ✅ `Blocked` overrides `AlwaysAllow` consent
- ✅ `OfflineOnly` denies all
- ✅ `LocalNetworkOnly` allows LAN, denies Internet
- ✅ Consent required for Internet destinations
- ✅ Destination classification (localhost, RFC1918, .local, public)

### 2.2 Consent Management

| Grant Type | Scope | Expiry |
|------------|-------|--------|
| `AllowOnce` | Single request | Consumed |
| `AllowForSession` | Session | App restart |
| `AlwaysAllow` | Persistent | Until revoked |
| `AlwaysDeny` | Persistent | Until revoked |

**Tests passed:**
- ✅ One-time grant consumed after use
- ✅ Session grant expires on reset
- ✅ Persistent allow/deny survive restarts
- ✅ Revoke clears all grant types
- ✅ Case-insensitive destination matching

---

## 3. Cryptography

| Component | Algorithm | Key Management |
|-----------|-----------|----------------|
| Memory encryption | AES-256-GCM | `KeyProvider` trait (FileKeyProvider interim) |
| Device pairing | X25519 ECDH + HKDF | QR code + 6-digit code |
| Transport encryption | AES-256-GCM | Per-session keys from pairing |
| Sync encryption | X25519 + AES-256-GCM | Device keypairs in SecurityManager |
| Certificates | ed25519 | Self-signed CA hierarchy |

**No `vendored-openssl` / SQLCipher** — all pure Rust (`aes-gcm`, `x25519-dalek`, `ed25519-dalek`, `ring`).

---

## 4. Module-Level Security

| Module | Attack Surface | Mitigations |
|--------|----------------|-------------|
| `nova_memory` | Encrypted DB file | AES-GCM, KeyProvider seam for OS keystore |
| `nova_search` | Derived index (plaintext) | Same future whole-DB encryption path |
| `nova_ai` | Model inference | Offline-first, remote seam consent-gated |
| `nova_voice` | Audio pipeline | 100% mock in CI, no mic access in tests |
| `nova_vision` | Image processing | No network, local decode only |
| `nova_knowledge` | Graph + entities | Local only, no external calls |
| `nova_pairing` | QR + code exchange | X25519, user-approved, no auto-pair |
| `nova_transport` | TCP/UDP | Encrypted, authenticated, heartbeat |
| `nova_sync` | Clipboard/files/memory | E2E encrypted, permission tokens |
| `nova_plugin_sdk` | Plugins | Sandbox trait, permission tokens |
| `nova_automation` | Actions | ConsequenceGate (Low/Med/High + consent) |

---

## 5. Static Analysis Results

| Tool | Findings | Severity |
|------|----------|----------|
| `cargo audit` | 0 vulnerabilities | — |
| `cargo deny check` | 0 advisories | — |
| `cargo clippy -D warnings` | 0 errors | — |
| `cargo fmt --check` | 0 diffs | — |

---

## 6. Attack Scenarios Tested

| Scenario | Vector | Result |
|----------|--------|--------|
| Unknown device pairs | `nova_pairing` | ✅ Rejected (no code match) |
| Replay pairing request | QR/code reuse | ✅ Session one-time |
| Invalid signature | Transport packet | ✅ Decrypt fails, dropped |
| Expired key | Sync/transport | ✅ Key rotation enforced |
| Tampered packet | MITM on transport | ✅ AES-GCM auth tag fails |
| Permission escalation | Plugin requests `internet` with only `memory.read` | ✅ Denied by `PluginSandbox` |
| Unauthorized file access | Plugin `read` without `file.read` | ✅ Denied |
| Unauthorized clipboard | Module reads clipboard without token | ✅ Denied |
| Unauthorized memory | Module queries memory without `memory.read` | ✅ Denied |
| Egress without consent | AI tries `api.example.com` | ✅ Blocked by EgressGate |
| Policy override | Consent `AlwaysAllow` but policy `Blocked` | ✅ Policy wins |

---

## 7. Supply Chain

| Dependency | Version | Audit |
|------------|---------|-------|
| `aes-gcm` | 0.10 | ✅ |
| `x25519-dalek` | 2.0 | ✅ |
| `ed25519-dalek` | 2.1 | ✅ |
| `ring` | 0.17 | ✅ |
| `rusqlite` | 0.31 (bundled) | ✅ |
| `tokio` | 1.38 | ✅ |
| `parking_lot` | 0.12 | ✅ |

No known CVEs in dependency tree.

---

## 8. Compliance Notes

- **GDPR-ready**: Local-only, user controls all data, export/import supported
- **No telemetry**: `telemetry_enabled=false` by default, opt-in only
- **No cloud dependency**: All features work offline
- **Key rotation**: `SecurityManager::rotate_keys()` implemented

---

## 9. Verdict

**PASS** — Security posture meets M15.2 release criteria. All documented attack vectors mitigated. Real-device penetration testing recommended for M16+.

---

**Next Audit:** M16 (Cross-Device Platform — real network exposure)
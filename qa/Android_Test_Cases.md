# Android Test Cases — NOVA v0.18.5-m15.2

**Device:** _______________ (Model, Android Version)
**Tester:** _______________
**Date:** _______________
**Build:** _______________ (APK version / commit hash)

---

## 1. Installation & Launch

| ID | Test Case | Steps | Expected | Actual | Pass/Fail |
|----|-----------|-------|----------|--------|-----------|
| AND-001 | Cold Start | 1. Reboot device<br>2. Launch NOVA from app drawer | App starts < 3s, shows main screen | | |
| AND-002 | Warm Start | 1. Launch NOVA<br>2. Home → recent apps → NOVA | Resumes < 500ms, state preserved | | |
| AND-003 | Background/Foreground | 1. Launch NOVA<br>2. Home button<br>3. Recent apps → NOVA | Service survives, state restored | | |
| AND-004 | Rotation | 1. Launch NOVA<br>2. Rotate device 90°<br>3. Rotate back | UI adapts, no crash, state kept | | |
| AND-004b | Split Screen | 1. Launch NOVA<br>2. Enter split screen mode | UI adapts, functional | | |

---

## 2. Permissions

| ID | Test Case | Steps | Expected | Actual | Pass/Fail |
|----|-----------|-------|----------|--------|-----------|
| AND-010 | Camera Permission | 1. Launch NOVA<br>2. Trigger vision capture | Prompts for camera, works after grant | | |
| AND-011 | Microphone Permission | 1. Trigger voice pipeline | Prompts for mic, works after grant | | |
| AND-012 | Storage Permission | 1. Trigger file picker | Prompts for storage, works after grant | | |
| AND-013 | Notification Permission | 1. Trigger notification | Prompts for notification, shows in tray | | |
| AND-014 | Permission Denial | 1. Deny camera permission<br>2. Trigger vision | Graceful fallback, no crash | | |

---

## 3. Core Features

| ID | Test Case | Steps | Expected | Actual | Pass/Fail |
|----|-----------|-------|----------|--------|-----------|
| AND-020 | Memory Insert | 1. Create memory via UI<br>2. Search for it | Memory stored, searchable | | |
| AND-021 | Memory Search | 1. Insert 10 memories<br>2. Search by text/tags | Results < 800ms, relevant | | |
| AND-022 | Voice Pipeline | 1. Say wake word "NOVA"<br>2. Speak query | VAD→Wake→ASR→AI→TTS completes | | |
| AND-023 | Voice Barge-in | 1. Start TTS response<br>2. Say wake word again | TTS cancels, new listening starts | | |
| AND-024 | Vision Capture | 1. Take photo via app<br>2. Analyze | OCR/caption/detection runs | | |
| AND-025 | Gallery Picker | 1. Select image from gallery<br>2. Analyze | Image loaded, results shown | | |

---

## 4. Cross-Device (Android → Windows)

| ID | Test Case | Steps | Expected | Actual | Pass/Fail |
|----|-----------|-------|----------|--------|-----------|
| AND-030 | Pairing (QR) | 1. Windows shows QR<br>2. Android scans QR<br>3. Enter 6-digit code | Pairing succeeds, trusted device listed | | |
| AND-031 | Clipboard Sync (A→W) | 1. Copy text on Android<br>2. Paste on Windows | Text appears < 500ms | | |
| AND-032 | Clipboard Sync (W→A) | 1. Copy text on Windows<br>2. Paste on Android | Text appears < 500ms | | |
| AND-033 | File Transfer (A→W) | 1. Send 1MB photo from Android<br>2. Receive on Windows | File received, verified | | |
| AND-034 | File Transfer (W→A) | 1. Send 1MB doc from Windows<br>2. Receive on Android | File received, verified | | |
| AND-035 | Memory Sync | 1. Create memory on Android<br>2. Search on Windows | Memory appears < 2s | | |
| AND-036 | Automation Sync | 1. Trigger workflow on Android<br>2. Check Windows | Workflow executes | | |

---

## 5. Network Modes

| ID | Test Case | Steps | Expected | Actual | Pass/Fail |
|----|-----------|-------|----------|--------|-----------|
| AND-040 | Wi-Fi LAN | 1. Both on same Wi-Fi<br>2. Pair & test | Auto-discovery, all features work | | |
| AND-041 | Phone Hotspot | 1. Android hotspot ON<br>2. Windows connects to AP<br>3. Pair & test | Works without router | | |
| AND-042 | Offline Mode | 1. Airplane mode ON<br>2. Test local features | Memory, search, voice work locally | | |
| AND-043 | Reconnect | 1. Disconnect Wi-Fi<br>2. Reconnect | Auto-reconnect, resume sync | | |

---

## 5. Security & Permissions

| ID | Test Case | Steps | Expected | Actual | Pass/Fail |
|----|-----------|-------|----------|--------|-----------|
| AND-050 | Unknown Device Blocked | 1. Try pairing without QR scan | Rejected, no trust established | | |
| AND-051 | Replay Attack | 1. Reuse old pairing code | Rejected, session one-time | | |
| AND-052 | Plugin Sandbox | 1. Install plugin without internet perm<br>2. Plugin tries network | Blocked by PluginSandbox | | |
| AND-053 | Unauthorized Memory | 1. No `memory.read` token<br>2. Query memory | Denied | | |
| AND-054 | Unauthorized Clipboard | 1. No `clipboard.read` token<br>2. Read clipboard | Denied | | |

---

## 6. Battery & Stability

| ID | Test Case | Steps | Expected | Actual | Pass/Fail |
|----|-----------|-------|----------|--------|-----------|
| AND-060 | Idle Battery (1hr) | 1. Charge to 100%<br>2. Idle 1hr<br>3. Check battery | < 2% drain | | |
| AND-061 | Active Battery (1hr) | 1. Continuous voice sessions<br>2. Check battery | < 10% drain | | |
| AND-062 | Low RAM | 1. Dev Options → Simulate 2GB<br>3. Run all features | No OOM, graceful degradation | | |
| AND-063 | App Restore | 1. Kill app via recents<br>2. Relaunch | State restored, no data loss | | |
| AND-064 | Long Run (4hr) | 1. Continuous cross-device sync<br>2. Monitor | No crashes, no leaks | | |

---

## 7. Notifications & UI

| ID | Test Case | Steps | Expected | Actual | Pass/Fail |
|----|-----------|-------|----------|--------|-----------|
| AND-070 | Foreground Service | 1. Start voice session<br>2. Check notification tray | Persistent notification visible | | |
| AND-071 | Voice Notification | 1. Complete voice turn<br>2. Check tray | Result notification appears | | |
| AND-072 | Pairing Notification | 1. Complete pairing<br>2. Check tray | "Device paired" notification | | |

---

## Summary

| Category | Total | Passed | Failed | Blocked |
|----------|-------|--------|--------|---------|
| Installation & Launch | 5 | | | |
| Permissions | 5 | | | |
| Core Features | 6 | | | |
| Cross-Device | 7 | | | |
| Network Modes | 4 | | | |
| Security | 5 | | | |
| Battery & Stability | 5 | | | |
| Notifications & UI | 3 | | | |
| **TOTAL** | **42** | | | |

---

**Tester Signature:** _______________ **Date:** _______________

**Notes / Issues Found:**
________________________________________________________________________
________________________________________________________________________
________________________________________________________________________
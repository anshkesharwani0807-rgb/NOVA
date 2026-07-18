# Windows Test Cases — NOVA v0.18.5-m15.2

**Machine:** _______________ (Model, Windows Version, CPU, RAM)
**Tester:** _______________
**Date:** _______________
**Build:** _______________ (nova_desktop version / commit hash)

---

## 1. Installation & Launch

| ID | Test Case | Steps | Expected | Actual | Pass/Fail |
|----|-----------|-------|----------|--------|-----------|
| WIN-001 | Cold Start | 1. Reboot machine<br>2. Launch `nova_desktop.exe` | App starts < 3s, shows main window | | |
| WIN-002 | Warm Start | 1. Launch app<br>2. Close → relaunch | Opens < 500ms, state preserved | | |
| WIN-003 | Minimize/Restore | 1. Launch → minimize to tray<br>2. Click tray icon | Window restores, state kept | | |
| WIN-004 | Full Screen/Resize | 1. Launch → maximize<br>2. Resize manually | UI adapts, no layout break | | |
| WIN-005 | Multiple Windows | 1. Open Search tab<br>2. Open Memory tab in new window | Both functional, independent | | |

---

## 2. System Tray

| ID | Test Case | Steps | Expected | Actual | Pass/Fail |
|----|-----------|-------|----------|--------|-----------|
| WIN-010 | Tray Icon Visible | 1. Launch app | Icon appears in system tray | | |
| WIN-011 | Tray Menu | 1. Right-click tray icon | Menu: Show, Settings, Quit | | |
| WIN-012 | Show from Tray | 1. Minimize to tray<br>2. Click tray icon | Main window restores | | |
| WIN-013 | Quit from Tray | 1. Right-click tray → Quit | Clean shutdown, kernel stops | | |
| WIN-014 | Tray Tooltip | 1. Hover tray icon | Shows "NOVA — Ready" | | |

---

## 3. Clipboard

| ID | Test Case | Steps | Expected | Actual | Pass/Fail |
|----|-----------|-------|----------|--------|-----------|
| WIN-020 | Clipboard Sync (W→A) | 1. Copy text on Windows<br>2. Paste on Android | Text appears < 500ms | | |
| WIN-021 | Clipboard Sync (A→W) | 1. Copy text on Android<br>2. Paste on Windows (Ctrl+V) | Text appears < 500ms | | |
| WIN-022 | Rich Text Sync | 1. Copy formatted text (bold, links)<br>2. Paste on other device | Formatting preserved | | |
| WIN-023 | Image Clipboard | 1. Copy screenshot (Win+Shift+S)<br>2. Paste on Android | Image appears | | |

---

## 4. File Operations

| ID | Test Case | Steps | Expected | Actual | Pass/Fail |
|----|-----------|-------|----------|--------|-----------|
| WIN-030 | File Transfer (W→A) | 1. Drag file to NOVA → Send to Android<br>2. Receive on phone | File received, verified | | |
| WIN-031 | File Transfer (A→W) | 1. Send photo from Android<br>2. Save on Windows | File received, opens correctly | | |
| WIN-032 | File Transfer (10MB) | 1. Send 10MB file<br>2. Verify hash | Complete < 3s, verified | | |
| WIN-033 | File Picker | 1. Click "Attach File"<br>2. Select multiple | Files queued for transfer | | |
| WIN-034 | Drag & Drop | 1. Drag file onto NOVA window<br>2. Send | File queued, sends | | |

---

## 5. Notifications

| ID | Test Case | Steps | Expected | Actual | Pass/Fail |
|----|-----------|-------|----------|--------|-----------|
| WIN-040 | Toast Notification | 1. Trigger notification from Android<br>2. Check Windows | Toast appears, actionable | | |
| WIN-041 | Action Center | 1. Open Action Center (Win+N)<br>2. Check NOVA notifications | Listed, clickable | | |
| WIN-042 | Pairing Notification | 1. Complete pairing | "Device paired" toast | | |
| WIN-043 | File Received Notification | 1. Receive file from Android | "File received: name" toast | | |

---

## 6. Process Control (via WindowsAgent)

| ID | Test Case | Steps | Expected | Actual | Pass/Fail |
|----|-----------|-------|----------|--------|-----------|
| WIN-050 | Launch App | 1. NOVA → "Open VS Code" | VS Code launches | | |
| WIN-051 | Close App | 1. NOVA → "Close Notepad" | Notepad closes gracefully | | |
| WIN-052 | Kill Process | 1. NOVA → "Force close Chrome" | Chrome terminates | | |
| WIN-053 | List Processes | 1. NOVA → "List running apps" | List matches Task Manager | | |
| WIN-054 | Volume Control | 1. NOVA → "Set volume 50%" | System volume = 50% | | |
| WIN-055 | Brightness Control | 1. NOVA → "Set brightness 80%" | Display brightness = 80% | | |
| WIN-056 | Lock Screen | 1. NOVA → "Lock computer" | Win+L triggered | | |
| WIN-057 | Screenshot | 1. NOVA → "Take screenshot" | PNG saved, path returned | | |
| WIN-057b | Window Control | 1. NOVA → "Minimize all" | All windows minimize | | |

---

## 7. Audio

| ID | Test Case | Steps | Expected | Actual | Pass/Fail |
|----|-----------|-------|----------|--------|-----------|
| WIN-060 | Mic Input | 1. Voice pipeline: speak to Windows mic<br>2. Check ASR | Audio captured, transcribed | | |
| WIN-061 | Speaker Output | 1. Voice pipeline: TTS response<br>2. Check speakers | Audio plays clearly | | |
| WIN-062 | Device Switch | 1. Change default mic/speaker<br>2. Test pipeline | Uses new device | | |

---

## 8. Window Control

| ID | Test Case | Steps | Expected | Actual | Pass/Fail |
|----|-----------|-------|----------|--------|-----------|
| WIN-070 | Minimize All | 1. NOVA → "Minimize all windows" | All apps minimize | | |
| WIN-071 | Restore All | 1. NOVA → "Restore windows" | All apps restore | | |
| WIN-072 | Focus Window | 1. NOVA → "Focus Chrome" | Chrome gains focus | | |

---

## 9. Shutdown & Restart

| ID | Test Case | Steps | Expected | Actual | Pass/Fail |
|----|-----------|-------|----------|--------|-----------|
| WIN-080 | Graceful Shutdown | 1. Tray → Quit<br>2. Check kernel logs | All modules stop cleanly | | |
| WIN-081 | Force Close | 1. Task Manager → End NOVA<br>2. Relaunch | No corruption, state ok | | |
| WIN-082 | Sleep/Wake | 1. Put PC to sleep<br>2. Wake → check NOVA | Reconnects cross-device | | |
| WIN-083 | Restart OS | 1. Restart Windows<br>2. Auto-start NOVA | App starts, connects | | |

---

## 10. Network & Cross-Device

| ID | Test Case | Steps | Expected | Actual | Pass/Fail |
|----|-----------|-------|----------|--------|-----------|
| WIN-090 | Wi-Fi LAN Discovery | 1. Both on same Wi-Fi<br>2. Pair | Auto-discovers Android | | |
| WIN-091 | Phone Hotspot | 1. Android hotspot ON<br>2. Windows connects to AP<br>3. Pair | Works without router | | |
| WIN-092 | Offline Mode | 1. Disconnect Wi-Fi<br>2. Test local features | Memory, search work | | |
| WIN-093 | Reconnect | 1. Disconnect network 30s<br>2. Reconnect | Auto-reconnect, sync | | |
| WIN-094 | Key Rotation | 1. Settings → Rotate keys<br>2. Re-pair not needed | Keys rotated, session kept | | |

---

## Summary

| Category | Total | Passed | Failed | Blocked |
|----------|-------|--------|--------|---------|
| Installation & Launch | 5 | | | |
| System Tray | 5 | | | |
| Clipboard | 4 | | | |
| File Operations | 5 | | | |
| Notifications | 4 | | | |
| Process Control | 6 | | | |
| Audio | 3 | | | |
| Window Control | 3 | | | |
| Shutdown/Restart | 4 | | | |
| Network/Cross-Device | 5 | | | |
| **TOTAL** | **46** | | | |

---

**Tester Signature:** _______________ **Date:** _______________

**Notes / Issues Found:**
________________________________________________________________________
________________________________________________________________________
________________________________________________________________________
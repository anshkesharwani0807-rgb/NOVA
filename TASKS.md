# M8 Tasks: Android Shell

- [x] Extend `nova_ffi` C-ABI with memory CRUD, search, config, activity trail, egress, health, stats, count
- [x] Create `api/jni/` crate with `crate-type = ["cdylib"]`, `jni 0.21` dep
- [x] Implement 16 JNI entry points in `api/jni/src/lib.rs` (Java_com_example_nova_NovaCore_* naming)
- [x] Add serde derives to `Query`, `SearchMode`, `SortBy`, `IndexStats` for JSON serialization
- [x] Create Kotlin `NovaCore` object with `external fun` matching JNI bridge
- [x] Create `NovaService` foreground service (notification channel, START_STICKY)
- [x] Auto-start service from `NovaApplication.onCreate`
- [x] Add `ActivityTrail` + `Settings` routes to navigation graph
- [x] Create `ActivityTrailScreen` (activity trail + egress log from native core)
- [x] Create `SettingsScreen` (config editor, health report, stats, memory count)
- [x] Wire `onActivityTrailClick` in SearchScreen header
- [x] Update AndroidManifest with foreground service + permissions
- [x] Create `build_android.ps1` cross-compilation script
- [x] All 4 verification gates green
- [x] Project docs updated (BRAIN.md, ROADMAP.md, CHANGELOG.md, AI_CONTEXT.md, SESSION.md, TASKS.md)

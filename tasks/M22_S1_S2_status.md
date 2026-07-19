# M22 S1+S2 Status — Complete

## Completed Work
- **S1: Intention Parser** — `modules/automation/src/intention_parser.rs`
  - 18 intent types (OpenApplication, CloseApplication, Search, Navigate, Click, Type, Scroll, Drag, Swipe, Wait, Speak, FileAction, SystemAction, DeviceControl, BrowserAction, MultiStepGoal, DeviceControl, plus OpenURL consolidated into BrowserAction)
  - Rule-based NL parser with extract helpers, quoted text, from→to patterns
  - 56 unit tests, all passing
- **S2: Goal Registry** — `modules/automation/src/goal_registry.rs`
  - 18 built-in goals, custom registration, resolution (Exact/Alias/Synonym/Partial/NoMatch)
  - `DefaultGoalResolver`, `GoalRegistryInner`, parameter guessing, tiebreaking
  - 46 unit tests, all passing
- **lib.rs** — `mod` + `pub use` for both modules

## Verification
- `cargo clippy --workspace --all-targets -- -D warnings` — clean
- `cargo fmt --check` — clean
- `cargo test -p nova_automation --lib` — 402/402 pass
- Pre-existing `STATUS_ACCESS_VIOLATION` in `real_executors` integration tests (unrelated)

## Key Decisions
- `extract_action_target` does NOT treat `.` as sentence punctuation (preserves URLs/extensions)
- `extract_quoted` checked before `extract_action_target` in all intent matchers
- `try_file_action` prioritized before `try_open_app` for "open file X"
- `try_open_app` uses `starts_with` only (no `contains`) to avoid "restart system" false match
- `param_boost` (0.01) in exact match scoring breaks ties between `open_app` vs `open_settings`

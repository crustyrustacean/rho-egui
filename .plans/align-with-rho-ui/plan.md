# rho-egui: align with rho-ui + current rho capabilities

Goal: make rho-egui a viable daily driver by (a) closing the feature gap vs rho-ui
and (b) fixing protocol drift against the *current* rho build.

Ground truth read from `../rho-coding-agent` (`rho/src/rpc.rs` dispatch + `rho-protocol/src/types.rs`),
cross-checked against rho-ui (`../rho-ui`) inventory.

## 1. Protocol drift (breaks existing features)

| # | Issue | Evidence |
|---|---|---|
| D1 | `tool/result` reads `isError`; rho sends `is_error` (`#[serde(rename="is_error")]`, breaks camelCase) -> **every tool failure renders green "done"** | rho-egui `protocol.rs:108` vs rho-protocol `types.rs:508` |
| D2 | Extensions list reads `status`/`toolCount`; current shape is `{name, tools:[String]}` -> list shows bare names, no tool count | rho-egui `app.rs:385-411` vs rho `rpc.rs:1160-1172` |
| D3 | Sessions: rho also returns `sizeKb`; unused (add column) | rho `rpc.rs:1144-1149` |
| D4 | Stats: rho returns `compactionTokens` + `phaseTokens{}`; never shown. rho-ui shows cache-hit-rate | rho `wire_conversions.rs:40-47` |
| D5 | `getState` returns `provider` + `messageCount`; both unused. Footer shows no provider | rho `rpc.rs:804-815` |

## 2. Missing agent methods (rho-ui has them, rho-egui doesn't)

- `fork`, `listBranches`, `switchBranch {cursorId}`, `nameBranch {cursorId,name}` -> branch management UI
- `listTools` -> tools inspector modal ({name, description, risk, parameters})
- `getMessages` -> transcript replay after resume/switch (rho-ui ignores this; egui should not)

## 3. Fixes from the earlier review (must fix for daily driver)

- F1 Quit uses `std::process::exit(0)`, skipping `Drop for RhoAgent` -> orphaned child (app.rs:639)
- F2 Redirect box consumes Enter without focus check -> steals Enter from main input (chat_view.rs:145)
- F3 Modals don't close on Esc / click-away
- F4 `finalize_tool_call` matches by name, not identity -> concurrent same-tool calls mis-finalize (store.rs:87)
- F5 `CommonMarkCache::default()` re-created per frame -> no markdown caching (chat_view.rs:66)
- F6 README says `pollster`; process.rs uses std threads + mpsc

## Deliberately NOT doing

- No redo/undo (session log has no time travel; both UIs lack it)
- No syntax highlighting in tool output (parity with rho-ui)
- Branch switch still clears transcript unless getMessages replay lands and is verified
- `phaseTokens` displayed read-only, no new action

## Plan

1. `agent/protocol.rs` — fix D1; add RequestKind variants; add parse tests for new shapes
2. `agent/process.rs` — add fork/list_branches/switch_branch/name_branch/list_tools/get_messages
3. `ui/widgets.rs` — BranchEntry + ToolEntry globals
4. `ui/modals.rs` — branch modal, tools modal
5. `app.rs` — new RequestKind handlers, menu buttons, F1, F2, F3, F5
6. `util/formatting.rs` — cache-hit-rate, compaction/phase rows, sizeKb
7. `chat/store.rs` — F4
8. Tests + README + version bump

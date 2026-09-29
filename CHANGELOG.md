## [1.0.0] - 2026-09-29

### 🚀 Features

- Add context management (Compact/Clear/NewSession)
- Render stats/context as two-column tables; bump to 0.2.1
- Align with current rho, scale up text; bump to 0.3.0
- First-class UI — fonts, markdown rendering, menu restructure; v1.0.0

## [0.3.0] - 2026-09-28

### 🚀 Features

- Branch management (fork, switch, label)
- Tool inspector modal
- `getMessages` support
- Compaction and phase token reporting in stats

### 🐛 Bug Fixes

- *(protocol)* Read `is_error` as well as `isError` on `tool/result`; the
  snake_case key is what rho actually sends, so failing tool calls were
  rendering green
- Quit via `ViewportCommand::Close` instead of `process::exit`, so the rho
  child is killed and reaped
- Redirect box no longer steals Enter from the main input
- Modals close on Esc and click-away
- `finalize_tool_call` prefers a still-pending call when matching by name
- Hoist `CommonMarkCache` out of the per-frame path

## [0.2.1] and earlier

- Context management, two-column stats tables, initial GUI shell.

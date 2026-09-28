# rho-egui

A desktop GUI for the [rho](https://github.com/jeffs/rho) coding assistant, built with [egui](https://github.com/emilk/egui) (immediate mode GUI).

## Features

- **Chat interface** — scrollable, stick-to-bottom chat with Markdown rendering via `egui_commonmark`
- **Streaming responses** — real-time token-by-token streaming of agent replies
- **Tool calls** — inline display of tool calls, results, and approval requests
- **Approval flow** — Approve, Deny, or Redirect tool calls with custom instructions
- **Model management** — pick from available models, filter by name or provider
- **Session management** — list, resume, and switch between sessions
- **Branch management** — fork, switch, and label session branches
- **Tool inspector** — browse registered tool schemas, risk levels, and parameters
- **Provider info** — view configured providers and their reachability
- **Usage tracking** — token counts, cost, cache hit rate, and context window utilization in the footer
- **Extensions** — list and reload rho extensions at runtime

## Getting Started

### Prerequisites

- [rho](https://github.com/jeffs/rho) installed and on your PATH (or set `RHO_PATH`)
- Rust toolchain (edition 2024)

### Run

```bash
cargo run
```

The app spawns a `rho` subprocess and communicates with it over JSON-RPC on stdin/stdout.

## Architecture

```
rho-egui
├── src/
│   ├── main.rs          — Entry point, eframe setup
│   ├── app.rs           — Main app state, event handling, UI layout
│   ├── agent.rs         — RhoAgent: subprocess management (module root)
│   ├── agent/
│   │   ├── process.rs   — Subprocess stdio via reader threads + mpsc
│   │   └── protocol.rs  — JSON-RPC notification parsing, RhoEvent enum
│   ├── chat.rs          — ChatBlock types, ApprovalResolution, ToolStatus
│   ├── chat/
│   │   └── store.rs     — In-memory chat store with streaming support
│   ├── ui.rs            — Module root
│   ├── ui/
│   │   ├── chat_view.rs — Block rendering, approval controls, redirect box
│   │   ├── modals.rs    — Model/session/provider/branch/tool pickers, stats, help
│   │   └── widgets.rs   — Shared state for model/session/provider/branch/tool lists
│   ├── util.rs          — Module root
│   └── util/
│       ├── formatting.rs — Duration formatting, session stats formatting
│       └── json.rs       — Safe JSON field access helpers
```

## Key Bindings

| Key | Action |
|---|---|
| `Enter` | Send message / Submit redirect (when the box has focus) |
| `Shift+Enter` | Insert newline |
| `Esc` | Close the open modal |
| `Ctrl+=` / `Ctrl+-` | Increase / decrease text size (this session) |

## Text Size

egui's defaults are small — 13pt body text, 9pt for `Small` — so rho-egui
scales every text style by **1.45×** and floors `Small` at 12pt. Markdown
responses inherit this automatically.

Tune it without recompiling:

```bash
RHO_TEXT_ZOOM=1.7 cargo run     # per-run override (0.5–4.0)
```

or adjust live with `Ctrl+=` / `Ctrl+-`.

**One trap worth knowing about if you edit this code.** The style tweak runs
every frame, so it must be *idempotent* — a tweak that clones the live style
out of the context and scales it in place compounds each frame
(13 → 18.9 → 27 → …), which presents as a hang or flicker rather than an
obvious "font bug". `apply_text_scale` therefore takes its baselines from a
fresh `egui::Style::default()` every time. `app.rs` has tests that assert this
property; if you change the function, keep them passing.

## Known Limitations

- The chat store is a flat, in-memory list with no persistence, so a resumed
  session's history is not replayed — only messages streamed in this process
  appear. Switching branches clears the transcript for the same reason.
- Tool calls are matched to their results by name only (rho's protocol sends
  no call id), so two concurrent calls to the same tool can share a result.
- Not available in the egui build: undo/redo, syntax-highlighted tool output,
  and text selection in modals.

## License

MIT
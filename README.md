# rho-egui

A desktop GUI for the [rho](https://github.com/jeffs/rho) coding assistant, built with [egui](https://github.com/emilk/egui) (immediate mode GUI).

## Features

- **Chat interface** — scrollable, stick-to-bottom chat with Markdown rendering via `egui_commonmark`
- **Streaming responses** — real-time token-by-token streaming of agent replies
- **Tool calls** — inline display of tool calls, results, and approval requests
- **Approval flow** — Approve, Deny, or Redirect tool calls with custom instructions
- **Model management** — pick from available models, filter by name or provider
- **Session management** — list, resume, and switch between sessions
- **Provider info** — view configured providers and their reachability
- **Usage tracking** — token counts, cost, and context window utilization in the footer
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
│   ├── agent/
│   │   ├── mod.rs       — RhoAgent: subprocess management
│   │   ├── process.rs   — Async stdin/stdout I/O via pollster
│   │   └── protocol.rs  — JSON-RPC notification parsing, RhoEvent enum
│   ├── chat/
│   │   ├── mod.rs       — ChatBlock types, ApprovalResolution, ToolStatus
│   │   └── store.rs     — In-memory chat store with streaming support
│   ├── ui/
│   │   ├── mod.rs
│   │   ├── chat_view.rs — Block rendering, approval controls, redirect box
│   │   ├── modals.rs    — Model picker, session picker, stats, help dialogs
│   │   └── widgets.rs   — Shared state for model/session/provider lists
│   └── util/
│       ├── mod.rs
│       ├── formatting.rs — Duration formatting, session stats formatting
│       └── json.rs       — Safe JSON field access helpers
```

## Key Bindings

| Key | Action |
|---|---|
| `Enter` | Send message / Submit redirect |
| `Shift+Enter` | Insert newline |
| `Ctrl+Enter` | *(future)* |

## License

MIT
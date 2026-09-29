# rho-egui

**The official UI for [rho](https://github.com/crustyrustacean/rho-coding-agent)** —
a desktop GUI built with [egui](https://github.com/emilk/egui) (immediate mode
GUI).

It spawns `rho` as a subprocess and talks to it over JSON-RPC 2.0 on
stdin/stdout, rendering the agent stream as a chat. `rho` itself is headless by
design; this is the front end.

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

- [rho](https://github.com/crustyrustacean/rho-coding-agent) installed and on your PATH (or set `RHO_PATH`)
- Rust toolchain (edition 2024)

### Install

Download a prebuilt binary from the
[releases page](https://github.com/crustyrustacean/rho-egui/releases) — Windows
and Linux x86-64 are built for every tag.

Or build from source:

```bash
cargo build --release
./target/release/rho-egui        # rho-egui.exe on Windows
```

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
  no call id), so two concurrent calls to the same tool can share a result. A
  pending call is preferred when resolving, which reduces but does not
  eliminate the ambiguity.
- Not available: undo/redo of prior turns.
- Tool output is plain monospace text, not syntax-highlighted. Code blocks
  *inside model responses* are highlighted; raw tool output is not.
- Responses are capped at 12,000 characters when collapsed, with an **Expand**
  button beyond that. Tool output caps at 10,000.

## Development

```bash
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test
```

CI runs all three on every push to `trunk`.

## Releasing

```bash
# pre-flight
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test

# bump, commit, tag
cargo install cargo-release
cargo release <version> --execute

# push — the tag triggers the binary build
git push origin trunk --tags
```

Pushing the tag runs `.github/workflows/release.yml`, which builds Windows and
Linux binaries and attaches them to the GitHub release with generated release
notes. `CHANGELOG.md` is generated from conventional commits with
[git-cliff](https://git-cliff.org):

```bash
git cliff --tag v1.0.0 --output CHANGELOG.md
```

## License

MIT
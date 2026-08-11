# Context Management for rho-egui

Add a "Context" menu button + modal matching the makepad rho-ui implementation.

## Changes

### 1. `src/agent/protocol.rs` — Add 3 new RequestKind variants
- `Compact` — maps to `"compact"` RPC method
- `Clear` — maps to `"clear"` RPC method  
- `NewSession` — maps to `"newSession"` RPC method

### 2. `src/agent/process.rs` — Add 3 new agent methods
- `compact()` — fire-and-forget `"compact"` request
- `clear()` — fire-and-forget `"clear"` request
- `new_session()` — fire-and-forget `"newSession"` request

### 3. `src/chat/store.rs` — Add `clear()` function
- `clear()` — clears the global CHAT_BLOCKS vec

### 4. `src/util/formatting.rs` — Add `format_usage_stats()` function
- Renders cached UsageState as aligned text for instant modal display while busy

### 5. `src/ui/modals.rs` — Add `ModalId::Context` variant + `context_modal()` function
- New modal with "Context management" title
- Shows stats body (same format as stats modal)
- Three action buttons: Compact, Clear, New Session
- Close button
- Returns picked action to caller (like session picker returns a picked session)

### 6. `src/app.rs` — Wire it all together
- Add "Context" button in menu bar
- Add `context_modal_requested: bool` field to App
- Button handler: when busy, render from cached usage; when idle, request stats and set flag
- Handle response: when `context_modal_requested` is set and `GetSessionStats` response arrives, format body and open modal
- Handle Compact/Clear/NewSession responses: push info block, refresh stats
- Handle modal action buttons: close modal + fire RPC

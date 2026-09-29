// src/chat/store.rs

use super::model::{ApprovalResolution, ChatBlock, ToolStatus};
use std::sync::RwLock;

static CHAT_BLOCKS: RwLock<Vec<ChatBlock>> = RwLock::new(Vec::new());

pub(crate) fn snapshot() -> Vec<ChatBlock> {
    CHAT_BLOCKS.read().unwrap().clone()
}

pub(crate) fn clear() {
    CHAT_BLOCKS.write().unwrap().clear();
}
pub(crate) fn push(block: ChatBlock) {
    CHAT_BLOCKS.write().unwrap().push(block);
}

pub(crate) fn push_after_finalizing_response(block: ChatBlock) {
    finalize_streaming_response();
    push(block);
}

pub(crate) fn append_streaming_response(delta: &str) {
    let mut blocks = CHAT_BLOCKS.write().unwrap();
    match blocks.last_mut() {
        Some(ChatBlock::ResponseStreaming(text)) => text.push_str(delta),
        _ => blocks.push(ChatBlock::ResponseStreaming(delta.to_string())),
    }
}

pub(crate) fn finalize_streaming_response() {
    let mut blocks = CHAT_BLOCKS.write().unwrap();
    if let Some(ChatBlock::ResponseStreaming(text)) = blocks.last_mut() {
        let owned = std::mem::take(text);
        *blocks.last_mut().unwrap() = ChatBlock::Response {
            text: owned,
            // Expanded by default: a finalized response is the thing the user
            // is waiting to read, so it should arrive formatted. The "Expand"
            // button appears only past RESPONSE_CAP, where the text is long
            // enough that collapsing is a real saving.
            expanded: true,
        };
    }
}

pub(crate) fn append_streaming_reasoning(delta: &str) {
    let mut blocks = CHAT_BLOCKS.write().unwrap();
    match blocks.last_mut() {
        Some(ChatBlock::ReasoningStreaming { text }) => text.push_str(delta),
        _ => blocks.push(ChatBlock::ReasoningStreaming {
            text: delta.to_string(),
        }),
    }
}

pub(crate) fn finalize_reasoning(elapsed_secs: String) {
    let mut blocks = CHAT_BLOCKS.write().unwrap();
    if let Some(ChatBlock::ReasoningStreaming { text }) = blocks.last_mut() {
        let owned = std::mem::take(text);
        *blocks.last_mut().unwrap() = ChatBlock::Reasoning {
            text: owned,
            elapsed_secs,
        };
    }
}

pub(crate) fn push_final_response_if_missing(reply: String) {
    if reply.is_empty() {
        return;
    }
    let mut blocks = CHAT_BLOCKS.write().unwrap();
    if !matches!(blocks.last(), Some(ChatBlock::Response { .. })) {
        blocks.push(ChatBlock::Response {
            text: reply,
            expanded: true,
        });
    }
}

pub(crate) fn push_pending_tool_call(name: String, args: String) {
    push(ChatBlock::ToolCall {
        name,
        args,
        status: ToolStatus::Pending,
        output: None,
        expanded: false,
    });
}

/// Mark the tool call matching `name` as finished.
///
/// The wire protocol identifies a tool call by name only — there is no call
/// id — so when the same tool is invoked more than once concurrently a result
/// can't be matched to its call with certainty. Searching newest-first and
/// preferring a still-`Pending` block keeps the common case exact, and makes
/// the failure mode "two calls share one result" rather than "one call is left
/// spinning forever".
pub(crate) fn finalize_tool_call(name: &str, status: ToolStatus, output: Option<String>) {
    let mut blocks = CHAT_BLOCKS.write().unwrap();
    // First pass: the most recent call to this tool that is still pending.
    let target = blocks
        .iter()
        .rposition(|b| matches!(b, ChatBlock::ToolCall { name: n, status: ToolStatus::Pending, .. } if n == name))
        .or_else(|| {
            blocks
                .iter()
                .rposition(|b| matches!(b, ChatBlock::ToolCall { name: n, .. } if n == name))
        });
    if let Some(i) = target
        && let ChatBlock::ToolCall { args, .. } = &blocks[i]
    {
        let args = args.clone();
        blocks[i] = ChatBlock::ToolCall {
            name: name.to_string(),
            args,
            status,
            output,
            expanded: false,
        };
    }
}

pub(crate) fn push_approval(tool: String, arguments: String, risk: String) {
    push(ChatBlock::Approval {
        tool,
        arguments,
        risk,
        resolution: ApprovalResolution::Pending,
    });
}

pub(crate) fn resolve_approval(index: usize, resolution: ApprovalResolution) {
    let mut blocks = CHAT_BLOCKS.write().unwrap();
    if let Some(ChatBlock::Approval { resolution: r, .. }) = blocks.get_mut(index) {
        *r = resolution;
    }
}

pub(crate) fn toggle_expand(index: usize) {
    let mut blocks = CHAT_BLOCKS.write().unwrap();
    if let Some(block) = blocks.get_mut(index)
        && let ChatBlock::Response { expanded, .. } | ChatBlock::ToolCall { expanded, .. } = block
    {
        *expanded = !*expanded;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, MutexGuard, OnceLock};

    /// `CHAT_BLOCKS` is a process-wide static and Rust runs tests in parallel,
    /// so every test in this module serializes on this lock. Clearing at the
    /// start of each test is not enough on its own — two tests can interleave
    /// clear/push/assert against each other.
    static TEST_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

    fn lock() -> MutexGuard<'static, ()> {
        TEST_LOCK
            .get_or_init(|| Mutex::new(()))
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Take the lock and clear the store, returning the guard that holds it
    /// for the rest of the test.
    fn reset() -> MutexGuard<'static, ()> {
        let guard = lock();
        clear();
        guard
    }

    fn tool_names() -> Vec<(String, String)> {
        snapshot()
            .into_iter()
            .filter_map(|b| match b {
                ChatBlock::ToolCall { name, status, .. } => Some((name, format!("{status:?}"))),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn finalize_prefers_pending_over_completed() {
        let _guard = reset();
        push_pending_tool_call("read".into(), "{\"a\":1}".into());
        // First call resolves.
        finalize_tool_call("read", ToolStatus::Success, Some("one".into()));
        // A second, still-pending call to the same tool arrives.
        push_pending_tool_call("read".into(), "{\"b\":2}".into());
        // Its result must land on the pending one, not re-finish the first.
        finalize_tool_call("read", ToolStatus::Error, Some("two".into()));

        let names = tool_names();
        assert_eq!(names.len(), 2);
        assert_eq!(names[0].1, "Success", "first call keeps its own result");
        assert_eq!(names[1].1, "Error", "second call receives the new result");
    }

    #[test]
    fn finalize_preserves_args_of_matched_block() {
        let _guard = reset();
        push_pending_tool_call("grep".into(), "{\"pattern\":\"x\"}".into());
        finalize_tool_call("grep", ToolStatus::Success, None);
        match snapshot().into_iter().next().unwrap() {
            ChatBlock::ToolCall { args, output, .. } => {
                assert_eq!(args, "{\"pattern\":\"x\"}");
                assert!(output.is_none());
            }
            other => panic!("expected ToolCall, got {other:?}"),
        }
    }

    #[test]
    fn finalize_unknown_tool_is_a_noop() {
        let _guard = reset();
        push_pending_tool_call("read".into(), "{}".into());
        finalize_tool_call("never_called", ToolStatus::Success, Some("x".into()));
        // Should not have created or mutated anything.
        assert_eq!(tool_names().len(), 1);
        assert_eq!(tool_names()[0].1, "Pending");
    }

    #[test]
    fn streaming_response_finalizes_once() {
        let _guard = reset();
        append_streaming_response("hel");
        append_streaming_response("lo");
        assert!(matches!(
            snapshot().last(),
            Some(ChatBlock::ResponseStreaming(t)) if t == "hello"
        ));
        finalize_streaming_response();
        assert!(matches!(
            snapshot().last(),
            Some(ChatBlock::Response { text, .. }) if text == "hello"
        ));
    }

    #[test]
    fn approval_resolution_by_index() {
        let _guard = reset();
        push_approval("shell".into(), "ls".into(), "high".into());
        push_approval("read".into(), "x".into(), "low".into());
        resolve_approval(1, ApprovalResolution::Denied);
        let resolutions: Vec<_> = snapshot()
            .into_iter()
            .filter_map(|b| match b {
                ChatBlock::Approval { resolution, .. } => Some(format!("{resolution:?}")),
                _ => None,
            })
            .collect();
        assert_eq!(resolutions, vec!["Pending", "Denied"]);
    }
}

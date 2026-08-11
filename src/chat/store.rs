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
            expanded: false,
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
            expanded: false,
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

pub(crate) fn finalize_tool_call(name: &str, status: ToolStatus, output: Option<String>) {
    let mut blocks = CHAT_BLOCKS.write().unwrap();
    for block in blocks.iter_mut().rev() {
        if let ChatBlock::ToolCall { name: n, .. } = block
            && n == name {
                *block = ChatBlock::ToolCall {
                    name: name.to_string(),
                    args: std::mem::take(&mut match block {
                        ChatBlock::ToolCall { args, .. } => args.clone(),
                        _ => String::new(),
                    }),
                    status,
                    output,
                    expanded: false,
                };
                return;
            }
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
    if let Some(block) = blocks.get_mut(index) {
        match block {
            ChatBlock::Response { expanded, .. } | ChatBlock::ToolCall { expanded, .. } => {
                *expanded = !*expanded
            }
            _ => {}
        }
    }
}

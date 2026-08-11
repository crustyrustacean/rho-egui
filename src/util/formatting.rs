// src/util/formatting.rs

use super::json::{jf64, ju64};
use serde_json::Value;
use std::time::{SystemTime, UNIX_EPOCH};

pub(crate) fn format_secs(ms: u64) -> String {
    format!("{:.1}s", ms as f64 / 1000.0)
}

/// Compact, timezone-free recency label for a unix-epoch timestamp (seconds).
pub(crate) fn relative_time(secs: u64) -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0);
    let delta = now.saturating_sub(secs);
    if delta < 60 {
        "just now".to_string()
    } else if delta < 3600 {
        format!("{}m ago", delta / 60)
    } else if delta < 86_400 {
        format!("{}h ago", delta / 3600)
    } else if delta < 86_400 * 7 {
        format!("{}d ago", delta / 86_400)
    } else if delta < 86_400 * 30 {
        format!("{}w ago", delta / (86_400 * 7))
    } else {
        format!("{}mo ago", delta / (86_400 * 30))
    }
}

/// Return the first `max` characters and note how many characters were omitted.
pub(crate) fn cap_head(value: &str, max: usize) -> String {
    let count = value.chars().count();
    if count <= max {
        value.to_string()
    } else {
        let head: String = value.chars().take(max).collect();
        format!("{head}\n\u{2026} ({} more chars)", count - max)
    }
}

/// Return the last `max` characters and note how many earlier characters were omitted.
pub(crate) fn cap_tail(value: &str, max: usize) -> String {
    let count = value.chars().count();
    if count <= max {
        value.to_string()
    } else {
        let tail: String = value.chars().skip(count - max).collect();
        format!("\u{2026} ({} earlier chars)\n{tail}", count - max)
    }
}

/// Render a getSessionStats result as aligned text for the Stats modal.
pub(crate) fn format_session_stats(result: &Value) -> String {
    let api = result.get("apiUsage");
    let role = result.get("roleTokens");
    let resolution = result.get("resolutionTokens");
    let compact = |number: u64| {
        if number >= 1000 {
            format!("{:.1}k", number as f64 / 1000.0)
        } else {
            number.to_string()
        }
    };
    format!(
        "Context\n\
         \x20 window          {}\n\
         \x20 used            {} ({}%)\n\
         \x20 remaining       {}\n\
         \x20 completion rsv  {}\n\
         \n\
         Session\n\
         \x20 messages        {}\n\
         \x20 entries         {} ({} on path, {} compacted)\n\
         \n\
         Role tokens\n\
         \x20 system          {}\n\
         \x20 user            {}\n\
         \x20 assistant       {}\n\
         \x20 tool            {}\n\
         \n\
         Resolution\n\
         \x20 full            {}\n\
         \x20 outlined        {}\n\
         \x20 summarized      {}\n\
         \x20 pinned          {}\n\
         \n\
         API usage\n\
         \x20 input           {}\n\
         \x20 output          {}\n\
         \x20 cached          {}\n\
         \x20 total           {}\n\
         \x20 requests        {}\n\
         \x20 cost            ${:.4}",
        compact(ju64(Some(result), "contextWindow")),
        compact(ju64(Some(result), "estimatedUsed")),
        ju64(Some(result), "utilizationPercent"),
        compact(ju64(Some(result), "estimatedRemaining")),
        compact(ju64(Some(result), "completionReserve")),
        ju64(Some(result), "messageCount"),
        ju64(Some(result), "entryCount"),
        ju64(Some(result), "pathEntryCount"),
        ju64(Some(result), "compactedEntryCount"),
        compact(ju64(role, "system")),
        compact(ju64(role, "user")),
        compact(ju64(role, "assistant")),
        compact(ju64(role, "tool")),
        compact(ju64(resolution, "full")),
        compact(ju64(resolution, "outlined")),
        compact(ju64(resolution, "summarized")),
        compact(ju64(resolution, "pinned")),
        compact(ju64(api, "totalInputTokens")),
        compact(ju64(api, "totalOutputTokens")),
        compact(ju64(api, "totalCachedTokens")),
        compact(ju64(api, "totalTokens")),
        ju64(api, "requestCount"),
        jf64(api, "totalCost"),
    )
}

/// Render cached UsageState as aligned text for the Stats/Context modal.
///
/// Used when a turn is running and a fresh getSessionStats round-trip would
/// queue behind the agent loop. Covers the context + API usage sections that
/// UsageState tracks; omits role/resolution/session breakdown (not available
/// from the Usage notification).
pub(crate) fn format_usage_stats(
    input: u64,
    output: u64,
    cached: u64,
    cost: f64,
    ctx_used: u64,
    ctx_window: u64,
    util: u8,
) -> String {
    let k = |n: u64| {
        if n == 0 {
            "0".to_string()
        } else {
            format!("{:.1}k", n as f64 / 1000.0)
        }
    };
    let remaining = ctx_window.saturating_sub(ctx_used);
    format!(
        "Context\n\
         \x20 window          {}\n\
         \x20 used            {} ({})%\n\
         \x20 remaining       {}\n\
         \n\
         API usage (cumulative)\n\
         \x20 input           {}\n\
         \x20 output          {}\n\
         \x20 cached          {}\n\
         \x20 cost            ${:.4}\n\
         \n\
         Full breakdown available when idle.",
        k(ctx_window),
        k(ctx_used),
        util,
        k(remaining),
        k(input),
        k(output),
        k(cached),
        cost,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_secs_whole_second() {
        assert_eq!(format_secs(1000), "1.0s");
    }

    #[test]
    fn cap_head_truncates_with_suffix() {
        assert_eq!(cap_head("hello world", 5), "hello\n\u{2026} (6 more chars)");
    }

    #[test]
    fn cap_tail_truncates_with_prefix() {
        assert_eq!(
            cap_tail("hello world", 5),
            "\u{2026} (6 earlier chars)\nworld"
        );
    }

    fn now_secs() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
    }

    #[test]
    fn relative_time_just_now() {
        assert_eq!(relative_time(now_secs()), "just now");
    }
}

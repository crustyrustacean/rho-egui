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

/// A single label/value pair in a stats table.
#[derive(Clone, Debug)]
pub(crate) struct StatRow {
    pub label: String,
    pub value: String,
}

/// A titled group of label/value rows rendered as a two-column table.
#[derive(Clone, Debug)]
pub(crate) struct StatSection {
    pub title: String,
    pub rows: Vec<StatRow>,
}

/// Body content for the Stats/Context modal — either structured table
/// sections or free-form text (e.g. the extensions list).
#[derive(Clone, Debug)]
pub(crate) enum StatsContent {
    Sections(Vec<StatSection>),
    Text(String),
}
impl StatsContent {
    pub(crate) fn sections(sections: Vec<StatSection>) -> Self {
        StatsContent::Sections(sections)
    }
    pub(crate) fn text(text: String) -> Self {
        StatsContent::Text(text)
    }
}

/// Build a `getSessionStats` result into titled two-column sections
/// for the Stats/Context modal.
pub(crate) fn format_session_stats(result: &Value) -> Vec<StatSection> {
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

    let used = ju64(Some(result), "estimatedUsed");
    let util = ju64(Some(result), "utilizationPercent");

    vec![
        StatSection {
            title: "Context".into(),
            rows: vec![
                StatRow {
                    label: "window".into(),
                    value: compact(ju64(Some(result), "contextWindow")),
                },
                StatRow {
                    label: "used".into(),
                    value: format!("{} ({}%)", compact(used), util),
                },
                StatRow {
                    label: "remaining".into(),
                    value: compact(ju64(Some(result), "estimatedRemaining")),
                },
                StatRow {
                    label: "completion reserve".into(),
                    value: compact(ju64(Some(result), "completionReserve")),
                },
            ],
        },
        StatSection {
            title: "Session".into(),
            rows: vec![
                StatRow {
                    label: "messages".into(),
                    value: ju64(Some(result), "messageCount").to_string(),
                },
                StatRow {
                    label: "entries".into(),
                    value: format!(
                        "{} ({} on path, {} compacted)",
                        ju64(Some(result), "entryCount"),
                        ju64(Some(result), "pathEntryCount"),
                        ju64(Some(result), "compactedEntryCount"),
                    ),
                },
            ],
        },
        StatSection {
            title: "Role tokens".into(),
            rows: vec![
                StatRow { label: "system".into(), value: compact(ju64(role, "system")) },
                StatRow { label: "user".into(), value: compact(ju64(role, "user")) },
                StatRow { label: "assistant".into(), value: compact(ju64(role, "assistant")) },
                StatRow { label: "tool".into(), value: compact(ju64(role, "tool")) },
            ],
        },
        StatSection {
            title: "Resolution".into(),
            rows: vec![
                StatRow { label: "full".into(), value: compact(ju64(resolution, "full")) },
                StatRow { label: "outlined".into(), value: compact(ju64(resolution, "outlined")) },
                StatRow { label: "summarized".into(), value: compact(ju64(resolution, "summarized")) },
                StatRow { label: "pinned".into(), value: compact(ju64(resolution, "pinned")) },
            ],
        },
        StatSection {
            title: "API usage".into(),
            rows: vec![
                StatRow { label: "input".into(), value: compact(ju64(api, "totalInputTokens")) },
                StatRow { label: "output".into(), value: compact(ju64(api, "totalOutputTokens")) },
                StatRow { label: "cached".into(), value: compact(ju64(api, "totalCachedTokens")) },
                StatRow { label: "total".into(), value: compact(ju64(api, "totalTokens")) },
                StatRow { label: "requests".into(), value: ju64(api, "requestCount").to_string() },
                StatRow { label: "cost".into(), value: format!("${:.4}", jf64(api, "totalCost")) },
            ],
        },
    ]
}

/// Build cached `UsageState` into titled two-column sections for the
/// Stats/Context modal.
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
) -> Vec<StatSection> {
    let k = |n: u64| {
        if n == 0 {
            "0".to_string()
        } else {
            format!("{:.1}k", n as f64 / 1000.0)
        }
    };
    let remaining = ctx_window.saturating_sub(ctx_used);
    vec![
        StatSection {
            title: "Context".into(),
            rows: vec![
                StatRow { label: "window".into(), value: k(ctx_window) },
                StatRow { label: "used".into(), value: format!("{} ({}%)", k(ctx_used), util) },
                StatRow { label: "remaining".into(), value: k(remaining) },
            ],
        },
        StatSection {
            title: "API usage (cumulative)".into(),
            rows: vec![
                StatRow { label: "input".into(), value: k(input) },
                StatRow { label: "output".into(), value: k(output) },
                StatRow { label: "cached".into(), value: k(cached) },
                StatRow { label: "cost".into(), value: format!("${:.4}", cost) },
            ],
        },
    ]
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

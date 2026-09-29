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

/// Truncate to `max` characters, backing up to a line boundary so the partial
/// text stays well-formed markdown.
///
/// [`cap_head`] cuts at an exact character count, which lands inside a list
/// item, a code fence or a table row. The result is a wall of literal `**` and
/// unclosed fences that reads as broken output rather than a preview.
///
/// Two rules, applied in order:
///   1. never end on an odd number of ``` fences — that would leave a code
///      block unterminated, so back up past the opening fence;
///   2. otherwise end at the last line break, provided it is not so far back
///      that little text would remain.
pub(crate) fn cap_head_lines(value: &str, max: usize) -> String {
    let count = value.chars().count();
    if count <= max {
        return value.to_string();
    }
    let head: String = value.chars().take(max).collect();

    let mut cut = match head.rfind('\n') {
        Some(idx) if idx >= MIN_KEPT_CHARS.min(max) => idx,
        // No usable line break: fall back to the last whitespace so a long
        // line does not end mid-word.
        Some(_) => last_whitespace(&head).unwrap_or(max),
        None => last_whitespace(&head).unwrap_or(max),
    };

    // A dangling ``` fence renders as an open code block to the end of the
    // message. Back up to before the opening fence when there is one.
    if count_fences(&head[..cut]) % 2 == 1
        && let Some(idx) = head[..cut].rfind("```")
    {
        cut = head[..idx].rfind('\n').unwrap_or(0);
    }

    // Never end on a bare list marker: a trailing "-" renders as an empty
    // bullet with nothing in it, which looks like a rendering fault.
    if let Some(idx) = head[..cut].rfind('\n') {
        let line = &head[idx + 1..cut];
        if line.len() <= 2 && line.chars().all(|c| c == '-' || c == '*' || c == '+') {
            cut = idx;
        }
    }

    let kept: String = head.chars().take(cut).collect();
    let kept_count = kept.chars().count();
    let omitted = format_thousands(count.saturating_sub(kept_count));
    format!("{kept}\n\n[...{omitted} more chars...]")
}

/// Never keep fewer than this many characters when backing up to a boundary.
const MIN_KEPT_CHARS: usize = 80;

fn last_whitespace(s: &str) -> Option<usize> {
    s.char_indices()
        .rev()
        .find(|(_, c)| c.is_whitespace())
        .map(|(i, _)| i)
}

/// Count ``` fence markers in `s`.
fn count_fences(s: &str) -> usize {
    s.matches("```").count()
}

/// Group digits: `1234567` -> `1,234,567`.
fn format_thousands(n: usize) -> String {
    let digits = n.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(ch);
    }
    out
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

/// Cache hit rate as a whole percentage of input tokens served from cache.
///
/// Returns `n/a` when no input tokens have been recorded, so a provider with
/// no prompt caching reads as *absent* rather than as a broken 0%. Some
/// providers over-report cached tokens (more cached than input), so the value
/// is clamped at 100%.
pub(crate) fn cache_hit_rate(input: u64, cached: u64) -> String {
    if input == 0 {
        return "n/a".to_string();
    }
    let pct = (cached as f64 / input as f64 * 100.0).min(100.0);
    format!("{pct:.0}%")
}

/// Build a `getSessionStats` result into titled two-column sections
/// for the Stats/Context modal.
pub(crate) fn format_session_stats(result: &Value) -> Vec<StatSection> {
    let api = result.get("apiUsage");
    let role = result.get("roleTokens");
    let resolution = result.get("resolutionTokens");
    let phase = result.get("phaseTokens");
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
                StatRow {
                    label: "system".into(),
                    value: compact(ju64(role, "system")),
                },
                StatRow {
                    label: "user".into(),
                    value: compact(ju64(role, "user")),
                },
                StatRow {
                    label: "assistant".into(),
                    value: compact(ju64(role, "assistant")),
                },
                StatRow {
                    label: "tool".into(),
                    value: compact(ju64(role, "tool")),
                },
            ],
        },
        StatSection {
            title: "Resolution".into(),
            rows: vec![
                StatRow {
                    label: "full".into(),
                    value: compact(ju64(resolution, "full")),
                },
                StatRow {
                    label: "outlined".into(),
                    value: compact(ju64(resolution, "outlined")),
                },
                StatRow {
                    label: "summarized".into(),
                    value: compact(ju64(resolution, "summarized")),
                },
                StatRow {
                    label: "pinned".into(),
                    value: compact(ju64(resolution, "pinned")),
                },
            ],
        },
        StatSection {
            title: "API usage".into(),
            rows: vec![
                StatRow {
                    label: "input".into(),
                    value: compact(ju64(api, "totalInputTokens")),
                },
                StatRow {
                    label: "output".into(),
                    value: compact(ju64(api, "totalOutputTokens")),
                },
                StatRow {
                    label: "cached".into(),
                    value: compact(ju64(api, "totalCachedTokens")),
                },
                StatRow {
                    label: "cache hit rate".into(),
                    value: cache_hit_rate(
                        ju64(api, "totalInputTokens"),
                        ju64(api, "totalCachedTokens"),
                    ),
                },
                StatRow {
                    label: "total".into(),
                    value: compact(ju64(api, "totalTokens")),
                },
                StatRow {
                    label: "requests".into(),
                    value: ju64(api, "requestCount").to_string(),
                },
                StatRow {
                    label: "cost".into(),
                    value: format!("${:.4}", jf64(api, "totalCost")),
                },
            ],
        },
        StatSection {
            title: "Compaction".into(),
            rows: vec![
                StatRow {
                    label: "compaction tokens".into(),
                    value: compact(ju64(Some(result), "compactionTokens")),
                },
                StatRow {
                    label: "compacted entries".into(),
                    value: ju64(Some(result), "compactedEntryCount").to_string(),
                },
            ],
        },
        StatSection {
            title: "Phase tokens".into(),
            rows: vec![
                StatRow {
                    label: "exploration".into(),
                    value: compact(ju64(phase, "exploration")),
                },
                StatRow {
                    label: "execution".into(),
                    value: compact(ju64(phase, "execution")),
                },
                StatRow {
                    label: "verification".into(),
                    value: compact(ju64(phase, "verification")),
                },
                StatRow {
                    label: "conclusion".into(),
                    value: compact(ju64(phase, "conclusion")),
                },
                StatRow {
                    label: "unclassified".into(),
                    value: compact(ju64(phase, "unclassified")),
                },
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
                StatRow {
                    label: "window".into(),
                    value: k(ctx_window),
                },
                StatRow {
                    label: "used".into(),
                    value: format!("{} ({}%)", k(ctx_used), util),
                },
                StatRow {
                    label: "remaining".into(),
                    value: k(remaining),
                },
            ],
        },
        StatSection {
            title: "API usage (cumulative)".into(),
            rows: vec![
                StatRow {
                    label: "input".into(),
                    value: k(input),
                },
                StatRow {
                    label: "output".into(),
                    value: k(output),
                },
                StatRow {
                    label: "cached".into(),
                    value: k(cached),
                },
                StatRow {
                    label: "cost".into(),
                    value: format!("${:.4}", cost),
                },
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

    // ── cap_head_lines ───────────────────────────────────────────────────

    #[test]
    fn cap_head_lines_passes_through_short_text() {
        assert_eq!(cap_head_lines("short", 100), "short");
    }

    #[test]
    fn cap_head_lines_backs_up_to_a_line_break() {
        // A hard cut at 28 would land inside the second list item and leave
        // the markdown half-open.
        let text = "# Title\n\n- alpha beta gamma\n- delta epsilon zeta";
        let out = cap_head_lines(text, 28);
        assert!(
            out.starts_with("# Title\n\n- alpha beta gamma"),
            "kept text should stop at the line break, got: {out:?}"
        );
    }

    #[test]
    fn cap_head_lines_never_ends_on_a_bare_list_marker() {
        // A 30-char budget lands right after the "-" of the second item.
        let text = "# Title\n\n- alpha beta gamma\n- delta epsilon zeta";
        let out = cap_head_lines(text, 30);
        let kept = out.split("\n\n[").next().unwrap();
        assert!(
            !kept.trim_end().ends_with('-'),
            "must not end on a dangling bullet, got: {kept:?}"
        );
    }

    #[test]
    fn cap_head_lines_does_not_split_an_unclosed_fence() {
        let text = "before\n\n```rust\nfn main() {}\nlet x = 1;\nlet y = 2;\n```\n\nafter";
        let out = cap_head_lines(text, 20);
        // The fence starts after the budget, so the prefix must not contain a
        // stray opening fence with no closing one.
        let opens = out.matches("```").count();
        assert_eq!(opens % 2, 0, "unbalanced code fence in: {out:?}");
    }

    #[test]
    fn cap_head_lines_keeps_most_of_a_single_long_line() {
        // No line break within reach of the budget, so we must not give back
        // almost the whole allowance looking for one.
        let text = "x".repeat(100);
        let out = cap_head_lines(&text, 40);
        let kept = out.split("\n\n[").next().unwrap();
        assert!(
            kept.len() >= 30,
            "single-line text should keep most of itself, kept {}",
            kept.len()
        );
    }

    #[test]
    fn cap_head_lines_reports_omitted_count() {
        let text = "a".repeat(2000);
        let out = cap_head_lines(&text, 100);
        assert!(out.contains("more chars"), "missing suffix: {out:?}");
    }

    #[test]
    fn format_thousands_groups_digits() {
        assert_eq!(format_thousands(0), "0");
        assert_eq!(format_thousands(999), "999");
        assert_eq!(format_thousands(1_000), "1,000");
        assert_eq!(format_thousands(1_234_567), "1,234,567");
    }
}

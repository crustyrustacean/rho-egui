// src/app.rs

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::agent::{RhoAgent, RhoEvent};
use crate::chat::{ApprovalResolution, ChatBlock, ToolStatus, store as chat_store};
use crate::ui::chat_view::{BlockAction, render_block};
use crate::ui::modals::{BranchAction, ContextAction, ModalId};
use crate::ui::widgets;
use crate::util::formatting::{
    format_secs, format_session_stats, format_usage_stats, StatsContent,
};
use crate::util::json::{jbool, jf64, jstr, jstr_opt, ju64};

/// Shorten an opaque cursor/entry id for display in a label.
fn short_id(id: &str) -> String {
    id.chars().take(8).collect()
}

// ── Text scaling ────────────────────────────────────────────────────────────

/// Default text-size multiplier.
///
/// egui's defaults are small: `Body` and `Button` are 13pt, `Small` is 9pt.
/// On a 100%-scale display that is genuinely hard to read. This is a
/// multiplier applied to egui's own baselines rather than hardcoded sizes, so
/// it keeps working if a future egui release changes them.
///
/// Override per-run with `RHO_TEXT_ZOOM` (e.g. `1.6`); `Ctrl+=` / `Ctrl+-`
/// adjust it live for the current session.
const DEFAULT_ZOOM: f32 = 1.45;

/// Floor applied to `TextStyle::Small`, which egui ships at 9pt — small enough
/// that proportional scaling alone can leave it unreadable.
const SMALL_FLOOR: f32 = 12.0;

/// Read the text zoom from the environment, falling back to [`DEFAULT_ZOOM`].
fn zoom_from_env() -> f32 {
    std::env::var("RHO_TEXT_ZOOM")
        .ok()
        .and_then(|raw| raw.trim().parse::<f32>().ok())
        .filter(|z| (0.5..=4.0).contains(z))
        .unwrap_or(DEFAULT_ZOOM)
}

/// Scale every text style in `style` by `zoom`, with a floor on `Small`.
///
/// Safe to call every frame. The trap this avoids: a style tweak that clones
/// the *live* style out of the context and scales it in place compounds on
/// every pass — 13 → 18.9 → 27 → … — because the scaled value is read back in
/// on the next frame. That is the runaway growth a previous attempt at this
/// fix produced, and it looks like a hang or a flicker rather than an
/// obvious "font size" bug.
///
/// The cure is to never scale a value that may already be scaled: take the
/// baselines from a fresh [`egui::Style::default()`] every time, so the
/// function is idempotent by construction.
///
/// Nothing here calls `request_repaint`, so it cannot feed a repaint loop.
fn apply_text_scale(style: &mut egui::Style, zoom: f32) {
    let base = egui::Style::default();
    for (text_style, base_font) in &base.text_styles {
        let size = base_font.size * zoom;
        let size = if *text_style == egui::TextStyle::Small {
            size.max(SMALL_FLOOR)
        } else {
            size
        };
        style.text_styles.insert(
            text_style.clone(),
            egui::FontId::new(size, base_font.family.clone()),
        );
    }
}

pub struct App {
    agent: Option<RhoAgent>,
    input_text: String,

    // Working / busy
    busy: bool,
    working_start: Option<Instant>,
    working_state: String,
    steer_count: u32,

    // Footer state
    usage: UsageState,
    current_model: String,
    current_provider: String,
    current_cwd: String,

    // Model list
    models: Vec<(String, String)>,
    model_filter_text: String,

    // Stats cache
    stats_body: StatsContent,

    // Modal control
    open_modal: ModalId,

    // "Resume Last" flag — next ListSessions response opens ResumeConfirm
    resume_latest_requested: bool,

    // Context modal flag — next GetSessionStats response opens Context modal
    context_modal_requested: bool,

    // Context modal body cache
    context_body: StatsContent,

    // Redirect input text per approval block index
    redirect_texts: HashMap<usize, String>,

    // Branch manager modal state
    branch_name_input: String,
    branch_selected: Option<usize>,

    // Have we spawned the agent yet?
    agent_spawned: bool,

    // Markdown layout cache. Hoisted out of the per-frame render path —
    // egui_commonmark reuses glyph layout keyed by this cache, so a fresh
    // `default()` each frame defeats all of it.
    md_cache: egui_commonmark::CommonMarkCache,

    /// Multiplier applied to egui's default text sizes. Adjustable at runtime
    /// with Ctrl+= / Ctrl+-; seeded from `RHO_TEXT_ZOOM`.
    text_zoom: f32,
}

#[derive(Default)]
struct UsageState {
    input: u64,
    output: u64,
    cached: u64,
    cost: f64,
    ctx_used: u64,
    ctx_window: u64,
    util: u8,
}

impl Default for App {
    fn default() -> Self {
        Self {
            agent: None,
            input_text: String::new(),
            busy: false,
            working_start: None,
            working_state: String::new(),
            steer_count: 0,
            usage: UsageState::default(),
            current_model: String::new(),
            current_provider: String::new(),
            current_cwd: String::new(),
            models: Vec::new(),
            model_filter_text: String::new(),
            stats_body: StatsContent::text(String::new()),
            open_modal: ModalId::None,
            resume_latest_requested: false,
            context_modal_requested: false,
            context_body: StatsContent::text(String::new()),
            redirect_texts: HashMap::new(),
            branch_name_input: String::new(),
            branch_selected: None,
            agent_spawned: false,
            md_cache: egui_commonmark::CommonMarkCache::default(),
            text_zoom: zoom_from_env(),
        }
    }
}

impl App {
    fn start_working(&mut self) {
        self.busy = true;
        self.working_start = Some(Instant::now());
        self.working_state.clear();
    }

    fn stop_working(&mut self) {
        self.busy = false;
        self.working_start = None;
        self.steer_count = 0;
    }

    fn push_block(&self, block: ChatBlock) {
        chat_store::push_after_finalizing_response(block);
    }

    fn format_usage(&self) -> String {
        let u = &self.usage;
        let k = |n: u64| {
            if n == 0 {
                "0".to_string()
            } else {
                format!("{:.1}k", n as f64 / 1000.0)
            }
        };
        if u.ctx_window == 0 && u.ctx_used == 0 {
            format!(
                "in {}    out {}    R {}    ${:.3}",
                k(u.input),
                k(u.output),
                k(u.cached),
                u.cost
            )
        } else {
            format!(
                "in {}    out {}    R {}    ${:.3}    ctx {}/{} ({}%)",
                k(u.input),
                k(u.output),
                k(u.cached),
                u.cost,
                k(u.ctx_used),
                k(u.ctx_window),
                u.util,
            )
        }
    }

    fn spawn_agent(&mut self, ctx: egui::Context) {
        self.agent_spawned = true;
        self.push_block(ChatBlock::Info("Starting rho…".into()));
        let repaint = Arc::new(move || ctx.request_repaint());
        match RhoAgent::spawn(repaint) {
            Ok(agent) => self.agent = Some(agent),
            Err(e) => {
                self.push_block(ChatBlock::Info(format!(
                    "⚠ Could not start rho: {e}\nSet RHO_PATH to the rho binary, or put rho on PATH.",
                )));
            }
        }
    }

    fn restart_agent(&mut self, ctx: egui::Context) {
        self.agent = None;
        self.stop_working();
        self.push_block(ChatBlock::Info("Restarting rho…".into()));
        let repaint = Arc::new(move || ctx.request_repaint());
        match RhoAgent::spawn(repaint) {
            Ok(agent) => self.agent = Some(agent),
            Err(e) => {
                self.push_block(ChatBlock::Info(format!("⚠ Could not restart rho: {e}")));
            }
        }
    }

    // ── Event handling ────────────────────────────────────────────────────

    fn handle_rho_event(&mut self, ev: RhoEvent) {
        match ev {
            RhoEvent::Ready => {
                self.push_block(ChatBlock::Info("✓ Connected to rho".into()));
                if let Some(agent) = &mut self.agent {
                    let _ = agent.get_state();
                    let _ = agent.list_models();
                    let _ = agent.get_session_stats();
                }
            }
            RhoEvent::AgentStart => self.start_working(),
            RhoEvent::MessageDelta { delta } => chat_store::append_streaming_response(&delta),
            RhoEvent::ReasoningDelta { delta } => {
                chat_store::finalize_streaming_response();
                chat_store::append_streaming_reasoning(&delta);
            }
            RhoEvent::AgentEnd { reply, duration_ms } => {
                chat_store::finalize_reasoning(format_secs(duration_ms));
                chat_store::finalize_streaming_response();
                chat_store::push_final_response_if_missing(reply);
                self.stop_working();
                if let Some(agent) = &mut self.agent {
                    let _ = agent.get_session_stats();
                }
            }
            RhoEvent::AgentError { error } => {
                self.push_block(ChatBlock::Info(format!("⚠ {error}")));
                self.stop_working();
            }
            RhoEvent::StateChange { state } => {
                self.working_state = state.clone();
                if state == "idle" && self.busy {
                    self.stop_working();
                }
            }
            RhoEvent::ToolCall { name, arguments } => {
                chat_store::finalize_streaming_response();
                chat_store::push_pending_tool_call(name, arguments);
            }
            RhoEvent::ToolResult {
                name,
                is_error,
                output,
            } => {
                let status = if is_error {
                    ToolStatus::Error
                } else {
                    ToolStatus::Success
                };
                let output = if output.is_empty() {
                    None
                } else {
                    Some(output)
                };
                chat_store::finalize_tool_call(&name, status, output);
            }
            RhoEvent::ToolDenied { name } => {
                chat_store::finalize_tool_call(
                    &name,
                    ToolStatus::Denied,
                    Some("denied by approval gate".into()),
                );
            }
            RhoEvent::ApprovalRequest {
                tool,
                arguments,
                risk,
            } => {
                chat_store::push_approval(tool, arguments, risk);
            }
            RhoEvent::Usage {
                input_tokens,
                output_tokens,
                cached_tokens,
                cost,
                context_used,
                context_window,
                utilization,
            } => {
                self.usage.input += input_tokens;
                self.usage.output += output_tokens;
                self.usage.cached += cached_tokens;
                self.usage.cost += cost;
                self.usage.ctx_used = context_used;
                self.usage.ctx_window = context_window;
                self.usage.util = utilization;
            }
            RhoEvent::Response { kind, result } => self.handle_response(kind, result),
            RhoEvent::RequestError { kind, error } => {
                self.push_block(ChatBlock::Info(format!("⚠ {kind:?} failed: {error}")));
            }
            RhoEvent::Closed => {
                self.push_block(ChatBlock::Info("rho process exited.".into()));
                self.agent = None;
                self.stop_working();
            }
        }
    }

    fn handle_response(&mut self, kind: crate::agent::RequestKind, result: serde_json::Value) {
        use crate::agent::RequestKind;
        match kind {
            RequestKind::GetState => {
                self.current_model = jstr(Some(&result), "model");
                self.current_cwd = jstr(Some(&result), "cwd");
                self.current_provider = jstr(Some(&result), "provider");
            }
            RequestKind::ListModels => {
                let mut models: Vec<(String, String)> = result
                    .get("models")
                    .and_then(|m| m.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|e| {
                                let id = e.get("id")?.as_str()?.to_owned();
                                Some((id, jstr(Some(e), "provider")))
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                models.sort_by(|a, b| a.1.cmp(&b.1).then_with(|| a.0.cmp(&b.0)));
                self.models = models;
                self.sync_model_list();
            }
            RequestKind::SetModel => {
                let model = jstr(Some(&result), "model");
                self.current_model = model.clone();
                self.push_block(ChatBlock::Info(format!("→ model: {model}")));
                if result.get("contextWindow").is_some() {
                    self.usage.ctx_window = ju64(Some(&result), "contextWindow");
                    self.usage.ctx_used = ju64(Some(&result), "estimatedUsed");
                    self.usage.util = ju64(Some(&result), "utilizationPercent").min(255) as u8;
                }
            }
            RequestKind::ListProviders => {
                let mut providers = Vec::new();
                if let Some(arr) = result.get("providers").and_then(|x| x.as_array()) {
                    for p in arr {
                        providers.push(widgets::ProviderEntry {
                            name: jstr(Some(p), "name"),
                            reachable: jbool(Some(p), "reachable"),
                            active: jbool(Some(p), "active"),
                            is_external: jbool(Some(p), "isExternal"),
                        });
                    }
                }
                if providers.is_empty() {
                    self.push_block(ChatBlock::Info("No providers configured.".into()));
                } else {
                    widgets::set_providers(providers);
                    self.open_modal = ModalId::ProviderInfo;
                }
            }
            RequestKind::ListSessions => {
                let mut sessions: Vec<widgets::SessionEntry> = result
                    .get("sessions")
                    .and_then(|x| x.as_array())
                    .map(|arr| {
                        arr.iter()
                            .map(|s| widgets::SessionEntry {
                                path: jstr(Some(s), "path"),
                                mtime_secs: ju64(Some(s), "mtimeSecs"),
                                entry_count: ju64(Some(s), "entryCount"),
                                size_kb: ju64(Some(s), "sizeKb"),
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                sessions.sort_by_key(|b| std::cmp::Reverse(b.mtime_secs));
                if sessions.is_empty() {
                    self.push_block(ChatBlock::Info("No previous sessions.".into()));
                } else if self.resume_latest_requested {
                    self.resume_latest_requested = false;
                    let newest = sessions[0].clone();
                    self.open_modal =
                        ModalId::ResumeConfirm(newest.path, newest.mtime_secs, newest.entry_count);
                } else {
                    widgets::set_sessions(sessions.clone());
                    self.open_modal = ModalId::SessionPicker;
                }
            }
            RequestKind::ResumeSession => {
                self.current_model = jstr(Some(&result), "model");
                self.current_cwd = jstr(Some(&result), "cwd");
                self.current_provider = jstr(Some(&result), "provider");
                self.push_block(ChatBlock::Info(format!(
                    "↻ resumed session ({})",
                    self.current_cwd
                )));
                // The resumed conversation is on disk but not on screen — the
                // chat store only ever holds what streamed in this process.
                // Ask rho for the active path and replay it as a summary.
                if let Some(agent) = &mut self.agent {
                    let _ = agent.get_messages();
                }
            }
            RequestKind::GetSessionStats => {
                let api = result.get("apiUsage");
                self.usage.input = ju64(api, "totalInputTokens");
                self.usage.output = ju64(api, "totalOutputTokens");
                self.usage.cached = ju64(api, "totalCachedTokens");
                self.usage.cost = jf64(api, "totalCost");
                self.usage.ctx_used = ju64(Some(&result), "estimatedUsed");
                self.usage.ctx_window = ju64(Some(&result), "contextWindow");
                self.usage.util = ju64(Some(&result), "utilizationPercent").min(255) as u8;
                self.stats_body = StatsContent::sections(format_session_stats(&result));
                // Context modal: same response feeds this modal
                // when the user opened it via the Context button.
                if self.context_modal_requested {
                    self.context_modal_requested = false;
                    self.context_body = self.stats_body.clone();
                    self.open_modal = ModalId::Context;
                }
            }
            RequestKind::ReloadExtensions => {
                let reloaded = ju64(Some(&result), "reloaded");
                let added = ju64(Some(&result), "added");
                let removed = ju64(Some(&result), "removed");
                self.push_block(ChatBlock::Info(format!(
                    "✓ extensions reloaded (+{added} ~{reloaded} -{removed})"
                )));
            }
            RequestKind::ListExtensions => {
                // Current shape is `{ name, tools: [String] }`; older rho sent
                // `status`/`toolCount`, which we no longer read. Fall back to
                // pretty-printing the whole response if the shape is unknown,
                // so a future schema change is visible rather than silent.
                let body = if let Some(arr) = result.get("extensions").and_then(|x| x.as_array()) {
                    if arr.is_empty() {
                        "No extensions installed.".to_string()
                    } else {
                        arr.iter()
                            .map(|e| {
                                let name = jstr(Some(e), "name");
                                let tools = e
                                    .get("tools")
                                    .and_then(|t| t.as_array())
                                    .map(|t| {
                                        t.iter()
                                            .filter_map(|n| n.as_str())
                                            .collect::<Vec<_>>()
                                    })
                                    .unwrap_or_default();
                                if tools.is_empty() {
                                    format!("  {name}")
                                } else {
                                    format!("  {name} ({} tools)", tools.len())
                                }
                            })
                            .collect::<Vec<_>>()
                            .join("\n")
                    }
                } else {
                    serde_json::to_string_pretty(&result).unwrap_or_default()
                };
                self.stats_body = StatsContent::text(body);
                self.open_modal = ModalId::Stats;
            }
            RequestKind::Fork => {
                // A fork registers a new cursor but leaves the active one
                // alone, so the transcript on screen is still accurate.
                self.push_block(ChatBlock::Info(
                    "↳ forked a new branch (not active — switch to it to use it)".into(),
                ));
                if let Some(agent) = &mut self.agent {
                    let _ = agent.list_branches();
                }
            }
            RequestKind::NameBranch => {
                // `name` is null when a blank name cleared the label.
                let label = jstr_opt(Some(&result), "name");
                self.push_block(ChatBlock::Info(match label {
                    Some(name) => format!("✓ branch labelled “{name}”"),
                    None => "✓ branch label cleared".into(),
                }));
                if let Some(agent) = &mut self.agent {
                    let _ = agent.list_branches();
                }
            }
            RequestKind::ListBranches => {
                let branches: Vec<widgets::BranchEntry> = result
                    .get("branches")
                    .and_then(|b| b.as_array())
                    .map(|arr| {
                        arr.iter()
                            .map(|b| {
                                let cursor_id = jstr(Some(b), "cursorId");
                                // `name` is null until the user labels it; fall
                                // back to a short id so rows are never blank.
                                let named = jstr_opt(Some(b), "name")
                                    .filter(|n| !n.trim().is_empty());
                                widgets::BranchEntry {
                                    label: named
                                        .clone()
                                        .unwrap_or_else(|| short_id(&cursor_id)),
                                    is_named: named.is_some(),
                                    cursor_id,
                                    active: jbool(Some(b), "active"),
                                }
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                widgets::set_branches(branches);
                self.open_modal = ModalId::Branches;
            }
            RequestKind::SwitchBranch => {
                let cursor_id = jstr(Some(&result), "cursorId");
                // The transcript is a flat list with no per-branch
                // partitioning, so the blocks on screen belong to the branch we
                // just left. Keeping them would misrepresent what the next
                // prompt sees, so clear and re-sync.
                chat_store::clear();
                self.push_block(ChatBlock::Info(format!(
                    "↪ switched to branch {}",
                    short_id(&cursor_id)
                )));
                self.branch_selected = None;
                if let Some(agent) = &mut self.agent {
                    let _ = agent.get_session_stats();
                    let _ = agent.get_messages();
                    let _ = agent.list_branches();
                }
            }
            RequestKind::ListTools => {
                let tools: Vec<widgets::ToolEntry> = result
                    .get("tools")
                    .and_then(|t| t.as_array())
                    .map(|arr| {
                        arr.iter()
                            .map(|t| widgets::ToolEntry {
                                name: jstr(Some(t), "name"),
                                description: jstr(Some(t), "description"),
                                risk: jstr(Some(t), "risk"),
                                parameters: t
                                    .get("parameters")
                                    .map(|p| serde_json::to_string_pretty(p).unwrap_or_default())
                                    .unwrap_or_default(),
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                widgets::set_tools(tools);
                self.open_modal = ModalId::Tools;
            }
            RequestKind::GetMessages => {
                // `messages` is the full active path, including the system
                // prompt. Replaying it verbatim would bury the live
                // transcript, so surface it as a count the user can act on.
                let n = result
                    .get("messages")
                    .and_then(|m| m.as_array())
                    .map(|a| a.len())
                    .unwrap_or(0);
                if n > 0 {
                    self.push_block(ChatBlock::Info(format!(
                        "· {n} message{} on the active path (not replayed)",
                        if n == 1 { "" } else { "s" }
                    )));
                }
            }
            RequestKind::Compact => {
                self.push_block(ChatBlock::Info("✓ compacted".into()));
                if let Some(agent) = &mut self.agent {
                    let _ = agent.get_session_stats();
                }
            }
            RequestKind::Clear => {
                chat_store::clear();
                self.push_block(ChatBlock::Info("✓ conversation cleared".into()));
                if let Some(agent) = &mut self.agent {
                    let _ = agent.get_session_stats();
                }
            }
            RequestKind::NewSession => {
                chat_store::clear();
                self.push_block(ChatBlock::Info("✓ new session".into()));
                if let Some(agent) = &mut self.agent {
                    let _ = agent.get_session_stats();
                }
            }
        }
    }

    fn sync_model_list(&mut self) {
        let current = self.current_model.as_str();
        let models: Vec<widgets::ModelEntry> = self
            .models
            .iter()
            .filter(|(id, _provider)| {
                let f = self.model_filter_text.to_lowercase();
                f.is_empty()
                    || id.to_lowercase().contains(&f)
                    || _provider.to_lowercase().contains(&f)
            })
            .map(|(id, provider)| widgets::ModelEntry {
                id: id.clone(),
                provider: provider.clone(),
                is_current: id.as_str() == current,
            })
            .collect();
        widgets::set_models(models);
    }

    fn submit_message(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        let steer = self.busy;
        chat_store::push(if steer {
            ChatBlock::Steer(text.to_string())
        } else {
            ChatBlock::User(text.to_string())
        });
        let result = if let Some(agent) = &mut self.agent {
            if steer {
                self.steer_count += 1;
                self.working_state = "steered".into();
                agent.prompt(text, true)
            } else {
                agent.prompt(text, false)
            }
        } else {
            Err("rho agent not connected.".into())
        };
        if let Err(e) = result {
            chat_store::push(ChatBlock::Info(format!("⚠ {e}")));
        }
    }

    fn resolve_approval(&mut self, index: usize, approved: bool, message: Option<String>) {
        let msg = message.filter(|m| !m.is_empty());
        let resolution = match (approved, &msg) {
            (true, _) => ApprovalResolution::Approved,
            (false, Some(m)) => ApprovalResolution::Redirected(m.clone()),
            (false, None) => ApprovalResolution::Denied,
        };
        chat_store::resolve_approval(index, resolution);
        if let Some(agent) = &mut self.agent {
            let _ = agent.approval_response(approved, msg);
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();

        // ── 0. Spawn agent on first frame ────────────────────────────────
        if !self.agent_spawned {
            self.spawn_agent(ctx.clone());
        }

        // ── 1. Drain agent events ─────────────────────────────────────────
        let events = self.agent.as_mut().map(|a| a.drain()).unwrap_or_default();
        for ev in events {
            self.handle_rho_event(ev);
        }

        // ── 2. Live text-zoom keys, then style tweaks ─────────────────────
        // Ctrl+= / Ctrl+- adjust text size without a restart. Handled before
        // the style is applied so the change lands in this same frame.
        {
            let (mut zoom_in, mut zoom_out) = (false, false);
            ctx.input_mut(|i| {
                zoom_in = i.consume_key(egui::Modifiers::CTRL, egui::Key::Equals)
                    || i.consume_key(egui::Modifiers::CTRL, egui::Key::Plus);
                zoom_out = i.consume_key(egui::Modifiers::CTRL, egui::Key::Minus);
            });
            if zoom_in {
                self.text_zoom = (self.text_zoom + 0.1).min(4.0);
            }
            if zoom_out {
                self.text_zoom = (self.text_zoom - 0.1).max(0.5);
            }
        }
        {
            let theme = egui::Theme::from_dark_mode(true);
            let mut style = ctx.style_of(theme).as_ref().clone();
            style.spacing.item_spacing.y = 0.0;
            style.spacing.button_padding = egui::vec2(6.0, 2.0);
            style.visuals.widgets.noninteractive.bg_fill =
                egui::Color32::from_rgb(0x0f, 0x0f, 0x12);
            style.visuals.panel_fill = egui::Color32::from_rgb(0x0f, 0x0f, 0x12);
            apply_text_scale(&mut style, self.text_zoom);
            ctx.set_style_of(theme, Arc::new(style));
        }

        // ── 3. Title bar ──────────────────────────────────────────────────
        egui::Panel::top("title_bar")
            .frame(egui::Frame {
                fill: egui::Color32::from_rgb(0x1b, 0x1b, 0x20),
                ..Default::default()
            })
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("rho")
                            .color(egui::Color32::from_rgb(0xea, 0xea, 0xea))
                            .size(18.0)
                            .strong(),
                    );
                    ui.colored_label(
                        egui::Color32::from_rgb(0x6a, 0x6a, 0x6a),
                        "Rust programming assistant",
                    );
                    // Show the active branch cursor, if it has a name, so the
                    // user can tell which conversation a prompt will extend.
                    // Unnamed branches fall back to a short id, which is
                    // noise in the title bar, so only show real labels.
                    if let Some(branch) = widgets::active_branch()
                        && branch.is_named
                    {
                        ui.colored_label(
                            egui::Color32::from_rgb(0x4f, 0x4f, 0x4f),
                            branch.label,
                        );
                    }
                });
            });

        // ── 4. Menu bar ───────────────────────────────────────────────────
        egui::Panel::top("menu_bar")
            .frame(egui::Frame {
                fill: egui::Color32::from_rgb(0x15, 0x15, 0x1a),
                ..Default::default()
            })
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    let btn =
                        |ui: &mut egui::Ui, label: &str| -> bool { ui.button(label).clicked() };
                    if btn(ui, "Session") {
                        if let Some(agent) = &mut self.agent {
                            let _ = agent.list_sessions();
                        } else {
                            self.push_block(ChatBlock::Info("not connected.".into()));
                        }
                    }
                    if btn(ui, "Resume Last") {
                        self.resume_latest_requested = true;
                        if let Some(agent) = &mut self.agent {
                            let _ = agent.list_sessions();
                        } else {
                            self.push_block(ChatBlock::Info("not connected.".into()));
                        }
                    }
                    if btn(ui, "Model") {
                        self.model_filter_text.clear();
                        self.sync_model_list();
                        self.open_modal = ModalId::ModelPicker;
                    }
                    if btn(ui, "Providers") {
                        if let Some(agent) = &mut self.agent {
                            let _ = agent.list_providers();
                        } else {
                            self.push_block(ChatBlock::Info("not connected.".into()));
                        }
                    }
                    if btn(ui, "Extensions") {
                        if let Some(agent) = &mut self.agent {
                            let _ = agent.list_extensions();
                        } else {
                            self.push_block(ChatBlock::Info("not connected.".into()));
                        }
                    }
                    if btn(ui, "Tools") {
                        if let Some(agent) = &mut self.agent {
                            let _ = agent.list_tools();
                        } else {
                            self.push_block(ChatBlock::Info("not connected.".into()));
                        }
                    }
                    if btn(ui, "Branches") {
                        self.branch_selected = None;
                        if let Some(agent) = &mut self.agent {
                            let _ = agent.list_branches();
                        } else {
                            self.push_block(ChatBlock::Info("not connected.".into()));
                        }
                    }
                    if btn(ui, "Reload") {
                        if let Some(agent) = &mut self.agent {
                            let _ = agent.reload_extensions();
                            self.push_block(ChatBlock::Info("Reloading extensions…".into()));
                        } else {
                            self.push_block(ChatBlock::Info("not connected.".into()));
                        }
                    }
                    if btn(ui, "Context") {
                        if self.busy {
                            // Render from cached usage data so the modal opens instantly
                            let u = &self.usage;
                            self.context_body = StatsContent::sections(format_usage_stats(
                                u.input, u.output, u.cached, u.cost,
                                u.ctx_used, u.ctx_window, u.util,
                            ));
                            self.open_modal = ModalId::Context;
                        } else {
                            match &mut self.agent {
                                Some(agent) => {
                                    self.context_modal_requested = true;
                                    let _ = agent.get_session_stats();
                                }
                                None => self.push_block(ChatBlock::Info("not connected.".into())),
                            }
                        }
                    }
                    if btn(ui, "Restart") {
                        self.restart_agent(ctx.clone());
                    }
                    if btn(ui, "Abort") {
                        if !self.busy {
                            self.push_block(ChatBlock::Info("nothing to abort.".into()));
                        } else if let Some(agent) = &mut self.agent {
                            let _ = agent.abort();
                            self.working_state = "aborting".into();
                        }
                    }
                    if btn(ui, "Stats") {
                        if let Some(agent) = &mut self.agent {
                            let _ = agent.get_session_stats();
                        }
                        self.open_modal = ModalId::Stats;
                    }
                    if btn(ui, "Help") {
                        self.open_modal = ModalId::Help;
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if btn(ui, "Quit") {
                            // Close the viewport rather than calling
                            // `process::exit`, which would skip `Drop for
                            // RhoAgent` and orphan the rho child (leaving it
                            // running with its stdin/stdout pipes still open).
                            // Drop then kills and reaps it cleanly.
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                    });
                });
            });

        // ── 5. Bottom panel: Working line + Input + Footer ────────────────
        egui::Panel::bottom("bottom_area")
            .frame(egui::Frame {
                fill: egui::Color32::from_rgb(0x0f, 0x0f, 0x12),
                ..Default::default()
            })
            .min_size(90.0)
            .show(ui, |ui| {
                // Working line
                if self.busy {
                    let elapsed = self.working_start.map(|s| s.elapsed()).unwrap_or_default();
                    const SPINNER: [char; 10] = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];
                    let ch = SPINNER[((elapsed.as_millis() / 80) as usize) % SPINNER.len()];
                    let state = if self.working_state.is_empty() {
                        "thinking".to_string()
                    } else {
                        self.working_state.clone()
                    };
                    let badge = if self.steer_count > 0 {
                        format!("  ↗ {} steered", self.steer_count)
                    } else {
                        String::new()
                    };
                    ui.colored_label(
                        egui::Color32::from_rgb(0xaa, 0xaa, 0x00),
                        format!(
                            "{ch} Working  {:.1}s  {state}{badge}",
                            elapsed.as_secs_f64()
                        ),
                    );
                    ctx.request_repaint_after(Duration::from_millis(80));
                }

                // Input box
                egui::Frame {
                    fill: egui::Color32::TRANSPARENT,
                    stroke: egui::Stroke::new(1.0, egui::Color32::from_rgb(0x4a, 0x4a, 0x4a)),
                    inner_margin: egui::Margin::symmetric(10, 8),
                    ..Default::default()
                }
                .show(ui, |ui| {
                    let placeholder = if self.busy {
                        "Steer the agent... (Enter to send, Shift+Enter for newline)"
                    } else {
                        "Type a message... (Enter to send, Shift+Enter for newline)"
                    };

                    let mut submit = false;

                    let resp = ui.add_sized(
                        egui::vec2(ui.available_width(), 50.0),
                        egui::TextEdit::multiline(&mut self.input_text)
                            .hint_text(placeholder)
                            .desired_width(f32::INFINITY)
                            .lock_focus(true)
                            .min_size(egui::vec2(ui.available_width(), 50.0)),
                    );

                    // Only consume keyboard events for the main input if it has focus,
                    // so the redirect text box can also use Enter/Shift+Enter.
                    let main_has_focus =
                        ui.memory(|m| m.focused() == Some(resp.id));
                    if main_has_focus {
                        if ctx
                            .input_mut(|i| i.consume_key(egui::Modifiers::SHIFT, egui::Key::Enter))
                        {
                            self.input_text.push('\n');
                        }
                        submit = ctx
                            .input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter));
                    }
                    if submit {
                        let text = std::mem::take(&mut self.input_text);
                        self.submit_message(&text);
                        resp.request_focus();
                    }
                });

                // Footer
                egui::Frame {
                    fill: egui::Color32::from_rgb(0x1b, 0x1b, 0x20),
                    inner_margin: egui::Margin::symmetric(12, 6),
                    ..Default::default()
                }
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.colored_label(
                            egui::Color32::from_rgb(0x6a, 0x6a, 0x6a),
                            &self.current_cwd,
                        );
                    });
                    ui.horizontal(|ui| {
                        ui.colored_label(
                            egui::Color32::from_rgb(0x6a, 0x6a, 0x6a),
                            self.format_usage(),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            // rho returns a separate `provider` on getState;
                            // prefix it so a provider-scoped model id stays legible.
                            if !self.current_provider.is_empty() {
                                ui.colored_label(
                                    egui::Color32::from_rgb(0x4f, 0x4f, 0x4f),
                                    &self.current_provider,
                                );
                            }
                            ui.colored_label(
                                egui::Color32::from_rgb(0x6a, 0x6a, 0x6a),
                                &self.current_model,
                            );
                        });
                    });
                });
            });

        // ── 6. Central panel: Chat scrollback ─────────────────────────────
        egui::CentralPanel::default()
            .frame(egui::Frame {
                fill: egui::Color32::from_rgb(0x0f, 0x0f, 0x12),
                inner_margin: egui::Margin::symmetric(12, 8),
                ..Default::default()
            })
            .show(ui, |ui| {
                let blocks = chat_store::snapshot();
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .stick_to_bottom(true)
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing.y = 4.0;
                        for (i, block) in blocks.iter().enumerate() {
                            // Split the borrow: `md_cache` and `redirect_texts`
                            // are disjoint from the block being rendered.
                            let App {
                                redirect_texts,
                                md_cache,
                                ..
                            } = self;
                            if let Some(action) =
                                render_block(ui, i, block, redirect_texts, md_cache)
                            {
                                match action {
                                    BlockAction::ExpandToggle(idx) => {
                                        chat_store::toggle_expand(idx)
                                    }
                                    BlockAction::Approve(idx) => {
                                        self.resolve_approval(idx, true, None)
                                    }
                                    BlockAction::Deny(idx) => {
                                        self.resolve_approval(idx, false, None)
                                    }
                                    BlockAction::Redirect(idx, msg) => {
                                        self.resolve_approval(idx, false, Some(msg))
                                    }
                                }
                            }
                        }
                    });
            });

        // ── 7. Modals ─────────────────────────────────────────────────────
        let open = self.open_modal.clone();
        if open != ModalId::None {
            let mut close_modal = false;
            let mut picked_model: Option<String> = None;
            let mut picked_session: Option<String> = None;

            let title = match &open {
                ModalId::ModelPicker => "Select model",
                ModalId::SessionPicker => "Resume session",
                ModalId::ProviderInfo => "Providers",
                ModalId::Stats => "Session stats",
                ModalId::Help => "Help",
                ModalId::Context => "Context management",
                ModalId::Tools => "Registered tools",
                ModalId::Branches => "Branches",
                ModalId::ResumeConfirm(..) => "Resume last session?",
                ModalId::None => unreachable!(),
            };
            // Modals that carry a selection close on outside click; the
            // context modal does NOT, because Compact/Clear/NewSession are
            // destructive and a stray click outside its buttons should not be
            // able to trigger them. Destructive modals therefore opt out here.
            let dismissible = !matches!(open, ModalId::Context | ModalId::ResumeConfirm(..));
            let mut open_flag = true;
            let mut win = egui::Window::new(title)
                .resizable(false)
                .collapsible(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .frame(egui::Frame {
                    fill: egui::Color32::from_rgb(0x1b, 0x1b, 0x20),
                    ..Default::default()
                });
            if dismissible {
                // Handing egui the flag enables its click-outside / Esc
                // dismissal; it writes `false` back when the user does so.
                win = win.open(&mut open_flag);
            }
            win.show(&ctx, |ui| {
                    match &open {
                        ModalId::ModelPicker => {
                            crate::ui::modals::model_picker(
                                ui,
                                &mut self.model_filter_text,
                                &mut picked_model,
                            );
                        }
                        ModalId::SessionPicker => {
                            crate::ui::modals::session_picker(ui, &mut picked_session);
                        }
                        ModalId::ProviderInfo => crate::ui::modals::provider_info(ui),
                        ModalId::Stats => crate::ui::modals::stats_modal(ui, &self.stats_body),
                        ModalId::Help => crate::ui::modals::help_modal(ui),
                        ModalId::Tools => crate::ui::modals::tools_modal(ui),
                        ModalId::ResumeConfirm(path, mtime, count) => {
                            crate::ui::modals::resume_confirm(ui, path, *mtime, *count);
                        }
                        ModalId::None => unreachable!(),
                        ModalId::Context => {} // handled below
                        ModalId::Branches => {}
                    }
                    // Context modal: action buttons return a ContextAction
                    if let ModalId::Context = &open {
                        if let Some(action) =
                            crate::ui::modals::context_modal(ui, &self.context_body)
                        {
                            close_modal = true;
                            if let Some(agent) = &mut self.agent {
                                match action {
                                    ContextAction::Compact => { let _ = agent.compact(); }
                                    ContextAction::Clear => { let _ = agent.clear(); }
                                    ContextAction::NewSession => { let _ = agent.new_session(); }
                                }
                            }
                        }
                    }
                    // Branch modal: returns a BranchAction
                    if let ModalId::Branches = &open {
                        let mut action: Option<BranchAction> = None;
                        crate::ui::modals::branch_modal(
                            ui,
                            &mut self.branch_name_input,
                            &mut self.branch_selected,
                            &mut action,
                        );
                        if let Some(action) = action {
                            if let Some(agent) = &mut self.agent {
                                match action {
                                    BranchAction::Fork => { let _ = agent.fork(); }
                                    BranchAction::Switch(i) => {
                                        if let Some(b) = widgets::get_branch(i) {
                                            let _ = agent.switch_branch(&b.cursor_id);
                                            // The modal reopens from the
                                            // ListBranches response, which
                                            // re-reads the active flag.
                                            self.branch_selected = None;
                                        }
                                    }
                                    BranchAction::Name(i, name) => {
                                        if let Some(b) = widgets::get_branch(i) {
                                            let _ = agent.name_branch(&b.cursor_id, &name);
                                        }
                                    }
                                }
                            }
                        }
                    }
                    ui.horizontal(|ui| {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            match &open {
                                ModalId::ResumeConfirm(path, ..) => {
                                    if ui.button("Cancel").clicked() {
                                        close_modal = true;
                                    }
                                    if ui.button("Resume").clicked() {
                                        if let Some(agent) = &mut self.agent {
                                            let _ = agent.resume_session(path);
                                        }
                                        close_modal = true;
                                    }
                                }
                                _ => {
                                    if ui.button("Close").clicked() {
                                        close_modal = true;
                                    }
                                }
                            }
                        });
                    });
                });

            // `open_flag` is flipped to false by egui when the user clicks
            // outside a dismissible window or presses Esc.
            if close_modal || !open_flag {
                self.open_modal = ModalId::None;
            }
            if let Some(model) = picked_model {
                if let Some(agent) = &mut self.agent {
                    let _ = agent.set_model(&model);
                }
                self.open_modal = ModalId::None;
            }
            if let Some(path) = picked_session {
                if let Some(agent) = &mut self.agent {
                    let _ = agent.resume_session(&path);
                }
                self.open_modal = ModalId::None;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The previous font-size fix grew without bound because the style tweak
    /// cloned the live style out of the context and scaled it in place, so
    /// each frame compounded on the last. `apply_text_scale` must be a no-op
    /// when handed a style it has already processed.
    #[test]
    fn apply_text_scale_is_idempotent() {
        let mut style = egui::Style::default();
        apply_text_scale(&mut style, DEFAULT_ZOOM);
        let first = style.text_styles.clone();
        for _ in 0..60 {
            apply_text_scale(&mut style, DEFAULT_ZOOM);
        }
        assert_eq!(
            style.text_styles, first,
            "re-applying the scale changed the result — this compounds every frame"
        );
    }

    /// Scaling must also survive a read-modify-write cycle, which is what the
    /// app does: take the style the context currently holds, tweak, set back.
    #[test]
    fn apply_text_scale_survives_read_modify_write() {
        let ctx = egui::Context::default();
        let theme = egui::Theme::Dark;
        let mut style = ctx.style_of(theme).as_ref().clone();
        apply_text_scale(&mut style, DEFAULT_ZOOM);
        ctx.set_style_of(theme, Arc::new(style));

        // Next frame reads it back and re-applies, exactly as `ui()` does.
        for _ in 0..60 {
            let mut style = ctx.style_of(theme).as_ref().clone();
            apply_text_scale(&mut style, DEFAULT_ZOOM);
            ctx.set_style_of(theme, Arc::new(style));
        }

        let final_style = ctx.style_of(theme);
        let body = final_style.text_styles[&egui::TextStyle::Body].size;
        let expected = egui::Style::default().text_styles[&egui::TextStyle::Body].size * DEFAULT_ZOOM;
        assert!(
            (body - expected).abs() < 0.01,
            "Body drifted to {body}, expected {expected} — the scale is compounding"
        );
    }

    /// Body must actually end up larger than egui's default, and `Small` must
    /// clear the legibility floor.
    #[test]
    fn apply_text_scale_enlarges_text_and_respects_floor() {
        let mut style = egui::Style::default();
        apply_text_scale(&mut style, DEFAULT_ZOOM);
        let base = egui::Style::default();

        let body = style.text_styles[&egui::TextStyle::Body].size;
        assert!(body > base.text_styles[&egui::TextStyle::Body].size);

        let small = style.text_styles[&egui::TextStyle::Small].size;
        assert!(
            small >= SMALL_FLOOR,
            "Small is {small}, below the {SMALL_FLOOR} floor"
        );
    }
}

// src/ui/widgets.rs

/// Shared state for model/session/provider pickers, read by both `app.rs`
/// (population) and `modals.rs` (rendering).
use std::sync::RwLock;

// ── Model list ────────────────────────────────────────────────────────────

#[derive(Clone)]
pub(crate) struct ModelEntry {
    pub id: String,
    pub provider: String,
    pub is_current: bool,
}

static MODELS: RwLock<Vec<ModelEntry>> = RwLock::new(Vec::new());

pub(crate) fn set_models(models: Vec<ModelEntry>) {
    *MODELS.write().unwrap() = models;
}

pub(crate) fn get_models() -> Vec<ModelEntry> {
    MODELS.read().unwrap().clone()
}

// ── Session list ──────────────────────────────────────────────────────────

#[derive(Clone)]
pub(crate) struct SessionEntry {
    pub path: String,
    pub mtime_secs: u64,
    pub entry_count: u64,
    /// On-disk size in whole KiB, as reported by rho's `listSessions`.
    pub size_kb: u64,
}

static SESSIONS: RwLock<Vec<SessionEntry>> = RwLock::new(Vec::new());

pub(crate) fn set_sessions(sessions: Vec<SessionEntry>) {
    *SESSIONS.write().unwrap() = sessions;
}

pub(crate) fn get_sessions() -> Vec<SessionEntry> {
    SESSIONS.read().unwrap().clone()
}

// ── Provider list ─────────────────────────────────────────────────────────

#[derive(Clone)]
pub(crate) struct ProviderEntry {
    pub name: String,
    pub reachable: bool,
    pub active: bool,
    pub is_external: bool,
}

static PROVIDERS: RwLock<Vec<ProviderEntry>> = RwLock::new(Vec::new());

pub(crate) fn set_providers(providers: Vec<ProviderEntry>) {
    *PROVIDERS.write().unwrap() = providers;
}

pub(crate) fn get_providers() -> Vec<ProviderEntry> {
    PROVIDERS.read().unwrap().clone()
}

// ── Branch list ────────────────────────────────────────────────────────────

/// A branch is a cursor over the shared session log. `active` marks the cursor
/// a subsequent `prompt` runs on.
#[derive(Clone)]
pub(crate) struct BranchEntry {
    pub cursor_id: String,
    pub label: String,
    pub active: bool,
    /// False when `label` is the short-id fallback rather than a user-assigned
    /// name. The fallback is fine for a picker row but is noise in a title bar.
    pub is_named: bool,
}

static BRANCHES: RwLock<Vec<BranchEntry>> = RwLock::new(Vec::new());

pub(crate) fn set_branches(branches: Vec<BranchEntry>) {
    *BRANCHES.write().unwrap() = branches;
}

pub(crate) fn get_branches() -> Vec<BranchEntry> {
    BRANCHES.read().unwrap().clone()
}

pub(crate) fn get_branch(index: usize) -> Option<BranchEntry> {
    BRANCHES.read().unwrap().get(index).cloned()
}

pub(crate) fn active_branch() -> Option<BranchEntry> {
    BRANCHES.read().unwrap().iter().find(|b| b.active).cloned()
}

// ── Tool list ──────────────────────────────────────────────────────────────

#[derive(Clone)]
pub(crate) struct ToolEntry {
    pub name: String,
    pub description: String,
    pub risk: String,
    /// Pretty-printed JSON schema for the tool's parameters.
    pub parameters: String,
}

static TOOLS: RwLock<Vec<ToolEntry>> = RwLock::new(Vec::new());

pub(crate) fn set_tools(tools: Vec<ToolEntry>) {
    *TOOLS.write().unwrap() = tools;
}

pub(crate) fn get_tools() -> Vec<ToolEntry> {
    TOOLS.read().unwrap().clone()
}

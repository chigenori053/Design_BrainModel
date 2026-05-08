//! UX Pipeline State Machine
//!
//! DBM-UX-GIT-PIPELINE-SPEC v1.0 §2/§5
//!
//! Tracks the Fix → Preview → Apply → GitAdd → Commit lifecycle as a
//! finite-state transaction, enforcing safe ordering and blocking
//! forbidden transitions (spec §2.3).

use std::path::PathBuf;

// ── PipelineState ─────────────────────────────────────────────────────────────

/// Current stage of the Fix → Apply → GitAdd → Commit pipeline.
///
/// Spec §2.1 状態定義 / DEM-CLI-PHASE1C5-SPEC §5.1
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum PipelineState {
    #[default]
    Idle,
    /// Proposal candidates have been presented; awaiting `select <n>`.
    /// Phase 1C.5 §5.1
    Proposed,
    Planned,
    Previewed,
    Applied,
    Staged,
    Committed,
}

impl PipelineState {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Idle => "Idle",
            Self::Proposed => "Proposed",
            Self::Planned => "Planned",
            Self::Previewed => "Previewed",
            Self::Applied => "Applied",
            Self::Staged => "Staged",
            Self::Committed => "Committed",
        }
    }

    /// `true` if `git add` is a valid next action.
    ///
    /// Spec §9.2 Add制約: requires Applied or Staged state.
    pub fn can_git_add(&self) -> bool {
        matches!(self, Self::Applied | Self::Staged)
    }

    /// `true` if `git commit` is a valid next action.
    ///
    /// Spec §2.3 禁止遷移: commit requires Staged state.
    pub fn can_commit(&self) -> bool {
        matches!(self, Self::Staged)
    }

    /// `true` if rollback is still available.
    ///
    /// Spec §7.2: rollback is forbidden after Committed.
    pub fn rollback_available(&self) -> bool {
        !matches!(self, Self::Committed)
    }

    pub fn can_transition_to(&self, next: &Self) -> bool {
        if self == next {
            return true;
        }
        self.rank() + 1 == next.rank()
    }

    fn rank(&self) -> u8 {
        match self {
            Self::Idle => 0,
            Self::Proposed => 1,
            Self::Planned => 2,
            Self::Previewed => 3,
            Self::Applied => 4,
            Self::Staged => 5,
            Self::Committed => 6,
        }
    }
}

impl std::fmt::Display for PipelineState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}

// ── PipelineContext ───────────────────────────────────────────────────────────

/// Mutable context for the Fix → Apply → GitAdd → Commit pipeline.
///
/// Spec §5.1 PipelineContext
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PipelineContext {
    pub state: PipelineState,
    /// Files modified by the last Apply.
    pub modified_files: Vec<PathBuf>,
    /// Files staged for the next commit.
    pub staged_files: Vec<PathBuf>,
    /// Short hash of the last commit, if any.
    pub last_commit: Option<String>,
}

impl PipelineContext {
    /// Transition to `Proposed` after execution candidates are generated.
    ///
    /// Phase 1C.5 §5.1: Idle → Proposed
    pub fn on_proposed(&mut self) {
        self.state = PipelineState::Proposed;
    }

    /// Transition to `Planned` after a plan is generated.
    ///
    /// Spec §3.1 Fix: Proposed → Planned.
    /// Direct-plan flows are an administrative fastpath, not semantic topology.
    pub fn on_planned(&mut self) {
        match self.state {
            PipelineState::Idle | PipelineState::Proposed => {
                self.state = PipelineState::Planned;
            }
            _ => {}
        }
    }

    /// Transition to `Previewed` after the diff is shown.
    ///
    /// Spec §3.2 Preview: Planned → Previewed
    /// Also accepts Idle → Previewed when preview is called without a prior
    /// explicit plan step.
    pub fn on_previewed(&mut self) {
        if matches!(self.state, PipelineState::Planned | PipelineState::Idle) {
            self.state = PipelineState::Previewed;
        }
    }

    /// Transition to `Applied` after a successful apply.
    ///
    /// Spec §3.3 Apply: Previewed → Applied
    pub fn on_applied(&mut self, modified: Vec<PathBuf>) {
        self.state = PipelineState::Applied;
        self.modified_files = modified;
        self.staged_files.clear();
    }

    /// Transition to `Staged` after a successful `git add`.
    ///
    /// Spec §3.4 GitAdd: Applied → Staged
    /// Duplicate paths are silently ignored.
    pub fn on_staged(&mut self, path: PathBuf) {
        if !self.staged_files.contains(&path) {
            self.staged_files.push(path);
        }
        self.state = PipelineState::Staged;
    }

    /// Transition to `Committed` after a successful `git commit`.
    ///
    /// Spec §3.5 Commit: Staged → Committed
    pub fn on_committed(&mut self, commit_hash: Option<String>) {
        self.state = PipelineState::Committed;
        self.last_commit = commit_hash;
    }

    /// Reset the pipeline back to `Idle` (e.g., after `/clear`).
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// Render the `[PIPELINE]` block for the status display.
    ///
    /// Spec §5.2 表示
    pub fn render(&self) -> String {
        let modified = format_path_list(&self.modified_files);
        let staged = format_path_list(&self.staged_files);
        format!(
            "state: {}\nmodified_files: {modified}\nstaged_files: {staged}",
            self.state
        )
    }

    /// Return `[NEXT]` action suggestions based on the current state.
    ///
    /// Spec §4.1 自動候補提示
    pub fn next_hints(&self) -> &'static [&'static str] {
        match self.state {
            PipelineState::Idle => &[],
            PipelineState::Proposed => &["select <n> で候補を選択"],
            PipelineState::Planned => &["preview で差分を確認"],
            PipelineState::Previewed => &["apply で変更を適用"],
            PipelineState::Applied => &[
                "git add <file> でファイルをステージング",
                "git commit でコミット",
            ],
            PipelineState::Staged => &["git commit でコミット"],
            PipelineState::Committed => &[],
        }
    }
}

fn format_path_list(paths: &[PathBuf]) -> String {
    if paths.is_empty() {
        "[]".to_string()
    } else {
        format!(
            "[{}]",
            paths
                .iter()
                .map(|p| p.display().to_string())
                .collect::<Vec<_>>()
                .join(", ")
        )
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initial_state_is_idle() {
        let ctx = PipelineContext::default();
        assert_eq!(ctx.state, PipelineState::Idle);
        assert!(ctx.modified_files.is_empty());
        assert!(ctx.staged_files.is_empty());
        assert!(ctx.last_commit.is_none());
    }

    /// Spec §12 完了条件: Fix→Apply→Add→Commit 正常系
    #[test]
    fn full_pipeline_happy_path() {
        let mut ctx = PipelineContext::default();

        ctx.on_planned();
        assert_eq!(ctx.state, PipelineState::Planned);

        ctx.on_previewed();
        assert_eq!(ctx.state, PipelineState::Previewed);

        ctx.on_applied(vec![PathBuf::from("src/main.rs")]);
        assert_eq!(ctx.state, PipelineState::Applied);
        assert_eq!(ctx.modified_files, vec![PathBuf::from("src/main.rs")]);
        assert!(ctx.staged_files.is_empty());

        ctx.on_staged(PathBuf::from("src/main.rs"));
        assert_eq!(ctx.state, PipelineState::Staged);
        assert_eq!(ctx.staged_files, vec![PathBuf::from("src/main.rs")]);

        ctx.on_committed(Some("a1b2c3d".to_string()));
        assert_eq!(ctx.state, PipelineState::Committed);
        assert_eq!(ctx.last_commit.as_deref(), Some("a1b2c3d"));
    }

    /// Spec §11: AddなしCommit拒否
    #[test]
    fn commit_requires_staged_state() {
        let mut ctx = PipelineContext::default();
        assert!(!ctx.state.can_commit(), "Idle must block commit");

        ctx.on_planned();
        assert!(!ctx.state.can_commit(), "Planned must block commit");

        ctx.on_previewed();
        assert!(!ctx.state.can_commit(), "Previewed must block commit");

        ctx.on_applied(vec![]);
        assert!(
            !ctx.state.can_commit(),
            "Applied (unstaged) must block commit"
        );

        ctx.on_staged(PathBuf::from("src/lib.rs"));
        assert!(ctx.state.can_commit(), "Staged must allow commit");
    }

    /// Spec §9.2 Add制約
    #[test]
    fn git_add_requires_applied_or_staged() {
        let mut ctx = PipelineContext::default();
        assert!(!ctx.state.can_git_add(), "Idle must block git add");
        ctx.on_planned();
        assert!(!ctx.state.can_git_add(), "Planned must block git add");
        ctx.on_previewed();
        assert!(!ctx.state.can_git_add(), "Previewed must block git add");
        ctx.on_applied(vec![]);
        assert!(ctx.state.can_git_add(), "Applied must allow git add");
        ctx.on_staged(PathBuf::from("a.rs"));
        assert!(ctx.state.can_git_add(), "Staged must allow further git add");
    }

    /// Spec §7.2: rollbackがcommit後不可
    #[test]
    fn rollback_unavailable_after_commit() {
        let mut ctx = PipelineContext::default();
        assert!(ctx.state.rollback_available());
        ctx.on_planned();
        assert!(ctx.state.rollback_available());
        ctx.on_previewed();
        assert!(ctx.state.rollback_available());
        ctx.on_applied(vec![]);
        assert!(ctx.state.rollback_available());
        ctx.on_staged(PathBuf::from("x.rs"));
        assert!(ctx.state.rollback_available());
        ctx.on_committed(None);
        assert!(
            !ctx.state.rollback_available(),
            "Committed must block rollback"
        );
    }

    #[test]
    fn reset_returns_to_idle() {
        let mut ctx = PipelineContext::default();
        ctx.on_planned();
        ctx.on_previewed();
        ctx.on_applied(vec![PathBuf::from("a.rs")]);
        ctx.reset();
        assert_eq!(ctx.state, PipelineState::Idle);
        assert!(ctx.modified_files.is_empty());
        assert!(ctx.staged_files.is_empty());
    }

    #[test]
    fn staged_files_deduplicated() {
        let mut ctx = PipelineContext::default();
        ctx.on_applied(vec![]);
        ctx.on_staged(PathBuf::from("a.rs"));
        ctx.on_staged(PathBuf::from("a.rs")); // duplicate
        assert_eq!(ctx.staged_files.len(), 1);
    }

    #[test]
    fn next_hints_match_state() {
        let mut ctx = PipelineContext::default();
        assert!(ctx.next_hints().is_empty(), "Idle: no hints");
        ctx.on_proposed();
        assert!(
            ctx.next_hints().iter().any(|h| h.contains("select")),
            "Proposed: must hint select"
        );
        ctx.on_planned();
        assert!(ctx.next_hints().iter().any(|h| h.contains("preview")));
        ctx.on_previewed();
        assert!(ctx.next_hints().iter().any(|h| h.contains("apply")));
        ctx.on_applied(vec![]);
        assert!(ctx.next_hints().iter().any(|h| h.contains("git add")));
        ctx.on_staged(PathBuf::from("a.rs"));
        assert!(ctx.next_hints().iter().any(|h| h.contains("git commit")));
        ctx.on_committed(None);
        assert!(ctx.next_hints().is_empty(), "Committed: no hints");
    }

    #[test]
    fn render_includes_state_and_files() {
        let mut ctx = PipelineContext::default();
        ctx.on_applied(vec![PathBuf::from("src/main.rs")]);
        ctx.on_staged(PathBuf::from("src/main.rs"));
        let rendered = ctx.render();
        assert!(rendered.contains("Staged"));
        assert!(rendered.contains("src/main.rs"));
    }

    #[test]
    fn non_contiguous_and_reverse_transitions_are_rejected() {
        // Idle → Proposed is valid (adjacent ranks)
        assert!(PipelineState::Idle.can_transition_to(&PipelineState::Proposed));
        // Proposed → Planned is valid (adjacent ranks)
        assert!(PipelineState::Proposed.can_transition_to(&PipelineState::Planned));
        // Idle → Planned is now a skip (Proposed sits between them)
        assert!(!PipelineState::Idle.can_transition_to(&PipelineState::Planned));
        assert!(!PipelineState::Idle.can_transition_to(&PipelineState::Applied));
        assert!(!PipelineState::Staged.can_transition_to(&PipelineState::Applied));
    }
}

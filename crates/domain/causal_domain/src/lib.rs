use std::{
    collections::{BTreeMap, BTreeSet, HashMap, VecDeque, hash_map::DefaultHasher},
    fs,
    hash::{Hash, Hasher},
    path::{Path, PathBuf},
};

pub const MIN_GRAPHS: usize = 2;
pub const MAX_GRAPHS: usize = 5;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub enum IntentType {
    #[default]
    Refactor,
    FixBug,
    Rename,
}

impl IntentType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Refactor => "refactor",
            Self::FixBug => "fix_bug",
            Self::Rename => "rename",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub enum ActionType {
    ExtractFunction,
    Inline,
    Rename,
    FixBug,
    RemoveCode,
}

impl ActionType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ExtractFunction => "extract_function",
            Self::Inline => "inline",
            Self::Rename => "rename",
            Self::FixBug => "fix_bug",
            Self::RemoveCode => "remove_code",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub enum Target {
    Unknown,
    Symbol(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ResolvedTarget {
    File(PathBuf),
    Function { file: PathBuf, name: String },
    SymbolUnknown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub enum Cause {
    ReadabilityLow,
    DuplicationHigh,
    BugPresent,
}

impl Cause {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ReadabilityLow => "readability_low",
            Self::DuplicationHigh => "duplication_high",
            Self::BugPresent => "bug_present",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub enum Goal {
    ImproveReadability,
    ReduceDuplication,
    RemoveBug,
}

impl Goal {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ImproveReadability => "improve_readability",
            Self::ReduceDuplication => "reduce_duplication",
            Self::RemoveBug => "remove_bug",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Action {
    pub action_type: ActionType,
    pub target: Target,
    pub params: HashMap<String, String>,
    pub confidence: f32,
}

impl Action {
    pub fn new(action_type: ActionType) -> Self {
        Self::with_confidence(action_type, 0.5)
    }

    pub fn with_confidence(action_type: ActionType, confidence: f32) -> Self {
        Self {
            action_type,
            target: Target::Unknown,
            params: HashMap::new(),
            confidence,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub enum Constraint {
    NoBehaviorChange,
    KeepPublicApi,
    ScopeLimited,
}

impl Constraint {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NoBehaviorChange => "no_behavior_change",
            Self::KeepPublicApi => "keep_public_api",
            Self::ScopeLimited => "scope_limited",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub enum CausalRelationKind {
    Enables,
    Inhibits,
    Requires,
    Emits,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub struct CausalRelation {
    pub target: u64,
    pub kind: CausalRelationKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub struct CausalEdge {
    pub from: u64,
    pub to: u64,
    pub kind: CausalRelationKind,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct CausalGraph {
    pub intent_id: String,
    pub intent_type: IntentType,
    pub causes: Vec<Cause>,
    pub goals: Vec<Goal>,
    pub constraints: Vec<Constraint>,
    pub actions: Vec<Action>,
    nodes: BTreeSet<u64>,
    edges: Vec<CausalEdge>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CausalValidation {
    pub valid: bool,
    pub issues: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ScoredGraph {
    pub graph: CausalGraph,
    pub score: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub enum IrOp {
    RefactorExtractFunction,
    RefactorInline,
    RefactorRename,
    FixBug,
    RemoveCode,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IrStep {
    pub op: IrOp,
    pub target: Target,
    pub params: HashMap<String, String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Change {
    Add {
        path: String,
        content: String,
    },
    Remove {
        path: String,
    },
    Modify {
        path: String,
        before: String,
        after: String,
    },
    InsertLine {
        file: PathBuf,
        line: usize,
        content: String,
    },
    DeleteLine {
        file: PathBuf,
        line: usize,
    },
    ReplaceBlock {
        file: PathBuf,
        start: usize,
        end: usize,
        content: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diff {
    pub changes: Vec<Change>,
    pub checksum: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExecutionStatus {
    PreviewOnly,
    Applied,
    Rejected,
    Failed,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExecutionResult {
    pub status: ExecutionStatus,
    pub diff: Option<Diff>,
    pub applied: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExecutionOptions {
    pub preview_only: bool,
    pub apply: bool,
    pub allow_destructive: bool,
    pub max_changes: usize,
    pub force_dry_run: bool,
    pub simulate_apply_failure: bool,
}

impl Default for ExecutionOptions {
    fn default() -> Self {
        Self {
            preview_only: true,
            apply: false,
            allow_destructive: false,
            max_changes: 50,
            force_dry_run: false,
            simulate_apply_failure: false,
        }
    }
}

impl CausalGraph {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_natural_language(input: &str) -> Vec<Self> {
        initial_causal_graphs_from_nl(input)
    }

    pub fn expand(graphs: Vec<Self>) -> Vec<Self> {
        expand_causal_graphs(graphs)
    }

    pub fn score(graphs: Vec<Self>) -> Vec<ScoredGraph> {
        score_causal_graphs(graphs)
    }

    pub fn select(scored_graphs: Vec<ScoredGraph>) -> Self {
        select_best_graph(scored_graphs)
    }

    pub fn to_ir(&self) -> Vec<IrStep> {
        causal_graph_to_ir(self)
    }

    pub fn is_ir_convertible_minimal(&self) -> bool {
        !self.intent_id.is_empty()
            && !self.causes.is_empty()
            && !self.goals.is_empty()
            && !self.actions.is_empty()
            && !self.constraints.is_empty()
    }

    pub fn add_node(&mut self, node: u64) {
        self.nodes.insert(node);
    }

    pub fn add_edge(&mut self, from: u64, to: u64, kind: CausalRelationKind) {
        self.nodes.insert(from);
        self.nodes.insert(to);
        self.edges.push(CausalEdge { from, to, kind });
    }

    pub fn nodes(&self) -> impl Iterator<Item = &u64> {
        self.nodes.iter()
    }

    pub fn edges(&self) -> &[CausalEdge] {
        &self.edges
    }

    pub fn closure_map(&self) -> BTreeMap<u64, BTreeSet<u64>> {
        self.nodes
            .iter()
            .map(|node| (*node, self.causal_closure(*node)))
            .collect()
    }

    pub fn causal_closure(&self, source: u64) -> BTreeSet<u64> {
        let mut visited = BTreeSet::new();
        let mut queue = VecDeque::from([source]);

        while let Some(current) = queue.pop_front() {
            for edge in self.edges.iter().filter(|edge| edge.from == current) {
                if visited.insert(edge.to) {
                    queue.push_back(edge.to);
                }
            }
        }

        visited
    }

    pub fn validate(&self) -> CausalValidation {
        let mut issues = Vec::new();

        if self.causes.is_empty() {
            issues.push("causes must not be empty".to_string());
        }
        if self.goals.is_empty() {
            issues.push("goals must not be empty".to_string());
        }
        if self.actions.is_empty() {
            issues.push("actions must not be empty".to_string());
        }
        if self.constraints.is_empty() {
            issues.push("constraints must not be empty".to_string());
        }

        for edge in &self.edges {
            if edge.from == edge.to {
                issues.push(format!("self causal edge detected at node {}", edge.from));
            }
        }

        for edge in &self.edges {
            if !self.nodes.contains(&edge.from) || !self.nodes.contains(&edge.to) {
                issues.push(format!(
                    "edge {} -> {} references an unknown node",
                    edge.from, edge.to
                ));
            }
        }

        for edge in &self.edges {
            if self.edges.iter().any(|other| {
                edge.from == other.to && edge.to == other.from && edge.kind != other.kind
            }) {
                issues.push(format!(
                    "conflicting causal edges detected between {} and {}",
                    edge.from, edge.to
                ));
            }
        }

        let closure = self.closure_map();
        for node in self.nodes() {
            if closure
                .get(node)
                .map(|reachable| reachable.contains(node))
                .unwrap_or(false)
            {
                issues.push(format!("causal cycle detected at node {}", node));
            }
        }

        issues.sort();
        issues.dedup();

        CausalValidation {
            valid: issues.is_empty(),
            issues,
        }
    }
}

pub fn initial_causal_graphs_from_nl(input: &str) -> Vec<CausalGraph> {
    let intent_type = extract_intent(input);
    let mut causes = infer_causes(intent_type);
    let mut goals = generate_goals(intent_type);
    let mut actions = generate_actions(intent_type);
    let mut constraints = default_constraints();

    if causes.is_empty() {
        causes.push(Cause::ReadabilityLow);
    }
    if goals.is_empty() {
        goals.push(Goal::ImproveReadability);
    }
    if actions.is_empty() {
        actions.push(Action::new(ActionType::ExtractFunction));
    }
    if constraints.is_empty() {
        constraints = default_constraints();
    }

    let index = 0;
    vec![CausalGraph {
        intent_id: deterministic_intent_id(input, index),
        intent_type,
        causes,
        goals,
        constraints,
        actions,
        nodes: BTreeSet::new(),
        edges: Vec::new(),
    }]
}

pub fn expand_causal_graphs(graphs: Vec<CausalGraph>) -> Vec<CausalGraph> {
    let source_graphs = if graphs.is_empty() {
        initial_causal_graphs_from_nl("")
    } else {
        graphs
    };

    let mut expanded = Vec::new();
    for graph in &source_graphs {
        let mut candidates = action_candidates_for_causes(&graph.causes);
        if candidates.is_empty() {
            candidates.push(action_for_expansion(ActionType::ExtractFunction));
        }

        for action in candidates {
            let index = expanded.len();
            let mut next = graph.clone();
            next.intent_id = deterministic_intent_id(&graph.intent_id, index);
            next.actions = vec![action];
            if next.validate().valid {
                expanded.push(next);
            }
            if expanded.len() >= MAX_GRAPHS {
                break;
            }
        }

        if expanded.len() >= MAX_GRAPHS {
            break;
        }
    }

    if expanded.is_empty() {
        return source_graphs;
    }

    while expanded.len() < MIN_GRAPHS {
        let Some(seed) = expanded.first().cloned() else {
            break;
        };
        let mut duplicate = seed;
        duplicate.intent_id = deterministic_intent_id(&duplicate.intent_id, expanded.len());
        if duplicate.validate().valid {
            expanded.push(duplicate);
        } else {
            break;
        }
    }

    expanded.truncate(MAX_GRAPHS);
    expanded
}

pub fn score_causal_graphs(graphs: Vec<CausalGraph>) -> Vec<ScoredGraph> {
    let mut scored: Vec<ScoredGraph> = graphs
        .into_iter()
        .map(|graph| {
            let score = score_graph(&graph);
            ScoredGraph { graph, score }
        })
        .collect();

    scored.sort_by(|lhs, rhs| {
        rhs.score
            .total_cmp(&lhs.score)
            .then_with(|| primary_action_order(&lhs.graph).cmp(&primary_action_order(&rhs.graph)))
    });
    scored
}

pub fn select_best_graph(scored_graphs: Vec<ScoredGraph>) -> CausalGraph {
    let mut valid_graphs: Vec<ScoredGraph> = scored_graphs
        .into_iter()
        .filter(|scored| scored.graph.validate().valid)
        .collect();

    valid_graphs.sort_by(|lhs, rhs| {
        rhs.score.total_cmp(&lhs.score).then_with(|| {
            selection_action_priority(&lhs.graph).cmp(&selection_action_priority(&rhs.graph))
        })
    });

    valid_graphs
        .into_iter()
        .next()
        .map(|scored| scored.graph)
        .unwrap_or_else(fallback_causal_graph)
}

pub fn causal_graph_to_ir(graph: &CausalGraph) -> Vec<IrStep> {
    if !graph.validate().valid {
        return fallback_ir();
    }

    let mut steps = Vec::new();
    for action in &graph.actions {
        steps.push(IrStep {
            op: action_type_to_ir_op(action.action_type),
            target: action.target.clone(),
            params: action.params.clone(),
        });
    }

    if steps.is_empty() {
        fallback_ir()
    } else {
        steps
    }
}

pub fn execute_ir_steps(steps: Vec<IrStep>) -> ExecutionResult {
    execute_ir_steps_with_options(steps, ExecutionOptions::default())
}

pub fn execute_ir_steps_with_options(
    steps: Vec<IrStep>,
    mut options: ExecutionOptions,
) -> ExecutionResult {
    if options.force_dry_run {
        options.apply = false;
    }

    if !pre_validate_ir(&steps) {
        return ExecutionResult {
            status: ExecutionStatus::Rejected,
            diff: None,
            applied: false,
        };
    }

    let resolved_targets = resolve_ir_targets(&steps);
    let diff = preview_diff(&steps);
    if !validate_diff(&diff, &resolved_targets, &options) {
        return ExecutionResult {
            status: ExecutionStatus::Rejected,
            diff: Some(diff),
            applied: false,
        };
    }

    if options.force_dry_run {
        return ExecutionResult {
            status: ExecutionStatus::PreviewOnly,
            diff: Some(diff),
            applied: false,
        };
    }

    if options.preview_only || !options.apply {
        return ExecutionResult {
            status: ExecutionStatus::PreviewOnly,
            diff: Some(diff),
            applied: false,
        };
    }

    if diff.checksum == 0 || diff.checksum != checksum_changes(&diff.changes) {
        return ExecutionResult {
            status: ExecutionStatus::Rejected,
            diff: Some(diff),
            applied: false,
        };
    }

    let mut state = HashMap::new();
    let before_apply = state.clone();
    if options.simulate_apply_failure
        || !apply_diff(&diff, &mut state)
        || !post_validate(&diff, &state)
        || checksum_changes(&diff.changes) != diff.checksum
    {
        state = before_apply;
        let _rolled_back = state;
        return ExecutionResult {
            status: ExecutionStatus::Failed,
            diff: Some(diff),
            applied: false,
        };
    }

    ExecutionResult {
        status: ExecutionStatus::Applied,
        diff: Some(diff),
        applied: true,
    }
}

pub fn score_graph(graph: &CausalGraph) -> f32 {
    let score = 0.5 * safety_score(graph) + 0.3 * action_score(graph) + 0.2 * goal_score(graph);
    if score.is_finite() {
        score.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

fn pre_validate_ir(steps: &[IrStep]) -> bool {
    !steps.is_empty() && steps.iter().all(|step| target_is_valid(&step.target))
}

pub fn resolve_target(target: &Target) -> ResolvedTarget {
    match target {
        Target::Unknown => ResolvedTarget::SymbolUnknown,
        Target::Symbol(symbol) => {
            let path = PathBuf::from(symbol);
            if path.is_file() {
                ResolvedTarget::File(path)
            } else {
                ResolvedTarget::SymbolUnknown
            }
        }
    }
}

fn resolve_ir_targets(steps: &[IrStep]) -> Vec<ResolvedTarget> {
    steps
        .iter()
        .map(|step| resolve_target(&step.target))
        .collect()
}

fn target_is_valid(target: &Target) -> bool {
    match target {
        Target::Unknown => true,
        Target::Symbol(symbol) => !symbol.trim().is_empty(),
    }
}

fn preview_diff(steps: &[IrStep]) -> Diff {
    let changes: Vec<Change> = steps.iter().map(preview_change).collect();
    let checksum = checksum_changes(&changes);
    Diff { changes, checksum }
}

fn preview_change(step: &IrStep) -> Change {
    if let ResolvedTarget::File(file) = resolve_target(&step.target) {
        return preview_file_change(step, file);
    }

    let path = deterministic_ir_path(step);
    match step.op {
        IrOp::RefactorExtractFunction => Change::Add {
            path,
            content: "refactor.extract_function".to_string(),
        },
        IrOp::RefactorInline => Change::Modify {
            path,
            before: "before.refactor_inline".to_string(),
            after: "after.refactor_inline".to_string(),
        },
        IrOp::RefactorRename => Change::Modify {
            path,
            before: "before.refactor_rename".to_string(),
            after: "after.refactor_rename".to_string(),
        },
        IrOp::FixBug => Change::Modify {
            path,
            before: "before.fix_bug".to_string(),
            after: "after.fix_bug".to_string(),
        },
        IrOp::RemoveCode => Change::Remove { path },
    }
}

fn preview_file_change(step: &IrStep, file: PathBuf) -> Change {
    let line_count = read_lines(&file).map(|lines| lines.len()).unwrap_or(0);
    match step.op {
        IrOp::RefactorExtractFunction => Change::InsertLine {
            file,
            line: line_count.saturating_add(1),
            content: "// dbm: extract function placeholder".to_string(),
        },
        IrOp::RefactorInline => Change::ReplaceBlock {
            file,
            start: 1,
            end: 1,
            content: "// dbm: inline preview".to_string(),
        },
        IrOp::RefactorRename => Change::ReplaceBlock {
            file,
            start: 1,
            end: 1,
            content: "// dbm: rename preview".to_string(),
        },
        IrOp::FixBug => Change::ReplaceBlock {
            file,
            start: 1,
            end: 1,
            content: "// dbm: fix bug preview".to_string(),
        },
        IrOp::RemoveCode => Change::DeleteLine { file, line: 1 },
    }
}

fn checksum_changes(changes: &[Change]) -> u64 {
    let mut hasher = DefaultHasher::new();
    changes.len().hash(&mut hasher);
    for change in changes {
        hash_change(change, &mut hasher);
    }
    let checksum = hasher.finish();
    if checksum == 0 { 1 } else { checksum }
}

fn hash_change(change: &Change, hasher: &mut DefaultHasher) {
    match change {
        Change::Add { path, content } => {
            0_u8.hash(hasher);
            path.hash(hasher);
            content.hash(hasher);
        }
        Change::Remove { path } => {
            1_u8.hash(hasher);
            path.hash(hasher);
        }
        Change::Modify {
            path,
            before,
            after,
        } => {
            2_u8.hash(hasher);
            path.hash(hasher);
            before.hash(hasher);
            after.hash(hasher);
        }
        Change::InsertLine {
            file,
            line,
            content,
        } => {
            3_u8.hash(hasher);
            file.hash(hasher);
            line.hash(hasher);
            content.hash(hasher);
        }
        Change::DeleteLine { file, line } => {
            4_u8.hash(hasher);
            file.hash(hasher);
            line.hash(hasher);
        }
        Change::ReplaceBlock {
            file,
            start,
            end,
            content,
        } => {
            5_u8.hash(hasher);
            file.hash(hasher);
            start.hash(hasher);
            end.hash(hasher);
            content.hash(hasher);
        }
    }
}

fn deterministic_ir_path(step: &IrStep) -> String {
    let target = match &step.target {
        Target::Unknown => "unknown".to_string(),
        Target::Symbol(symbol) => format!("symbol/{symbol}"),
    };
    format!("ir/{}/{}.plan", target, ir_op_name(step.op))
}

fn ir_op_name(op: IrOp) -> &'static str {
    match op {
        IrOp::RefactorExtractFunction => "refactor_extract_function",
        IrOp::RefactorInline => "refactor_inline",
        IrOp::RefactorRename => "refactor_rename",
        IrOp::FixBug => "fix_bug",
        IrOp::RemoveCode => "remove_code",
    }
}

fn validate_diff(
    diff: &Diff,
    resolved_targets: &[ResolvedTarget],
    options: &ExecutionOptions,
) -> bool {
    !diff.changes.is_empty()
        && diff.checksum != 0
        && diff.checksum == checksum_changes(&diff.changes)
        && diff.changes.len() <= options.max_changes
        && !(options.force_dry_run && options.apply)
        && (!options.apply
            || resolved_targets
                .iter()
                .all(|target| !matches!(target, ResolvedTarget::SymbolUnknown)))
        && diff
            .changes
            .iter()
            .all(|change| validate_change(change, options))
}

fn validate_change(change: &Change, options: &ExecutionOptions) -> bool {
    match change {
        Change::Add { .. } | Change::Modify { .. } => true,
        Change::Remove { .. } | Change::DeleteLine { .. } => options.allow_destructive,
        Change::InsertLine { file, line, .. } => validate_file_line(file, *line, true),
        Change::ReplaceBlock {
            file,
            start,
            end,
            content,
        } => {
            validate_file_range(file, *start, *end)
                && !content.trim_start().starts_with("pub ")
                && !range_contains_public_item(file, *start, *end)
                && end.saturating_sub(*start).saturating_add(1) <= options.max_changes
        }
    }
}

fn validate_file_line(file: &Path, line: usize, allow_eof_insert: bool) -> bool {
    let Ok(lines) = read_lines(file) else {
        return false;
    };
    let upper = if allow_eof_insert {
        lines.len().saturating_add(1)
    } else {
        lines.len()
    };
    line >= 1 && line <= upper
}

fn validate_file_range(file: &Path, start: usize, end: usize) -> bool {
    let Ok(lines) = read_lines(file) else {
        return false;
    };
    start >= 1 && start <= end && end <= lines.len()
}

fn apply_diff(diff: &Diff, state: &mut HashMap<String, String>) -> bool {
    for change in &diff.changes {
        match change {
            Change::Add { path, content } => {
                state.insert(path.clone(), content.clone());
            }
            Change::Modify { path, after, .. } => {
                state.insert(path.clone(), after.clone());
            }
            Change::Remove { path } => {
                state.remove(path);
            }
            Change::InsertLine {
                file,
                line,
                content,
            } => {
                if !apply_file_change(change, file) {
                    return false;
                }
                state.insert(
                    file.to_string_lossy().to_string(),
                    format!("insert:{line}:{content}"),
                );
            }
            Change::DeleteLine { file, line } => {
                if !apply_file_change(change, file) {
                    return false;
                }
                state.insert(file.to_string_lossy().to_string(), format!("delete:{line}"));
            }
            Change::ReplaceBlock {
                file,
                start,
                end,
                content,
            } => {
                if !apply_file_change(change, file) {
                    return false;
                }
                state.insert(
                    file.to_string_lossy().to_string(),
                    format!("replace:{start}:{end}:{content}"),
                );
            }
        }
    }
    true
}

fn post_validate(diff: &Diff, state: &HashMap<String, String>) -> bool {
    diff.changes.iter().all(|change| match change {
        Change::Add { path, content } => state.get(path) == Some(content),
        Change::Modify { path, after, .. } => state.get(path) == Some(after),
        Change::Remove { path } => !state.contains_key(path),
        Change::InsertLine {
            file,
            line,
            content,
        } => state
            .get(&file.to_string_lossy().to_string())
            .is_some_and(|value| value == &format!("insert:{line}:{content}")),
        Change::DeleteLine { file, line } => state
            .get(&file.to_string_lossy().to_string())
            .is_some_and(|value| value == &format!("delete:{line}")),
        Change::ReplaceBlock {
            file,
            start,
            end,
            content,
        } => state
            .get(&file.to_string_lossy().to_string())
            .is_some_and(|value| value == &format!("replace:{start}:{end}:{content}")),
    })
}

fn read_lines(path: &Path) -> std::io::Result<Vec<String>> {
    Ok(fs::read_to_string(path)?
        .lines()
        .map(ToString::to_string)
        .collect())
}

fn range_contains_public_item(file: &Path, start: usize, end: usize) -> bool {
    read_lines(file)
        .map(|lines| {
            lines
                .iter()
                .enumerate()
                .filter(|(index, _)| {
                    let line_number = index + 1;
                    line_number >= start && line_number <= end
                })
                .any(|(_, line)| line.trim_start().starts_with("pub "))
        })
        .unwrap_or(true)
}

fn apply_file_change(change: &Change, file: &Path) -> bool {
    let backup = backup_path(file);
    if fs::copy(file, &backup).is_err() {
        return false;
    }

    let result = apply_file_change_inner(change, file);
    if result.is_err() {
        let _ = fs::copy(&backup, file);
        return false;
    }

    true
}

fn apply_file_change_inner(change: &Change, file: &Path) -> std::io::Result<()> {
    let mut lines = read_lines(file)?;
    match change {
        Change::InsertLine { line, content, .. } => {
            let index = line.saturating_sub(1).min(lines.len());
            lines.insert(index, content.clone());
        }
        Change::DeleteLine { line, .. } => {
            let index = line.saturating_sub(1);
            if index >= lines.len() {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "delete line out of range",
                ));
            }
            lines.remove(index);
        }
        Change::ReplaceBlock {
            start,
            end,
            content,
            ..
        } => {
            let start_index = start.saturating_sub(1);
            let end_index = *end;
            if start_index >= lines.len() || end_index > lines.len() || start > end {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "replace range out of bounds",
                ));
            }
            lines.splice(start_index..end_index, [content.clone()]);
        }
        Change::Add { .. } | Change::Remove { .. } | Change::Modify { .. } => {}
    }

    atomic_write(file, &lines.join("\n"))
}

fn atomic_write(path: &Path, content: &str) -> std::io::Result<()> {
    let tmp = path.with_extension(format!(
        "{}tmp",
        path.extension()
            .and_then(|extension| extension.to_str())
            .map(|extension| format!("{extension}."))
            .unwrap_or_default()
    ));
    fs::write(&tmp, format!("{content}\n"))?;
    fs::rename(tmp, path)
}

fn backup_path(path: &Path) -> PathBuf {
    let mut backup = path.as_os_str().to_os_string();
    backup.push(".bak");
    PathBuf::from(backup)
}

fn action_type_to_ir_op(action_type: ActionType) -> IrOp {
    match action_type {
        ActionType::ExtractFunction => IrOp::RefactorExtractFunction,
        ActionType::Inline => IrOp::RefactorInline,
        ActionType::Rename => IrOp::RefactorRename,
        ActionType::FixBug => IrOp::FixBug,
        ActionType::RemoveCode => IrOp::RemoveCode,
    }
}

fn fallback_ir() -> Vec<IrStep> {
    vec![IrStep {
        op: IrOp::RefactorExtractFunction,
        target: Target::Unknown,
        params: HashMap::new(),
    }]
}

pub fn extract_intent(input: &str) -> IntentType {
    if input.contains("リファクタ") || input.contains("きれいに") {
        IntentType::Refactor
    } else if input.contains("直して") {
        IntentType::FixBug
    } else if input.contains("名前変更") {
        IntentType::Rename
    } else {
        IntentType::Refactor
    }
}

fn safety_score(graph: &CausalGraph) -> f32 {
    if graph
        .actions
        .iter()
        .any(|action| action.action_type == ActionType::RemoveCode)
        && graph.constraints.contains(&Constraint::NoBehaviorChange)
    {
        0.0
    } else {
        1.0
    }
}

fn action_score(graph: &CausalGraph) -> f32 {
    primary_action_type(graph)
        .map(score_action_type)
        .unwrap_or(0.0)
}

fn score_action_type(action_type: ActionType) -> f32 {
    match action_type {
        ActionType::ExtractFunction => 0.8,
        ActionType::Rename => 0.7,
        ActionType::Inline => 0.6,
        ActionType::FixBug => 0.9,
        ActionType::RemoveCode => 0.4,
    }
}

fn goal_score(graph: &CausalGraph) -> f32 {
    let Some(action_type) = primary_action_type(graph) else {
        return 0.0;
    };

    if graph
        .goals
        .iter()
        .any(|goal| goal_matches_action(*goal, action_type))
    {
        1.0
    } else {
        0.5
    }
}

fn goal_matches_action(goal: Goal, action_type: ActionType) -> bool {
    match goal {
        Goal::ImproveReadability => matches!(
            action_type,
            ActionType::ExtractFunction | ActionType::Rename | ActionType::Inline
        ),
        Goal::ReduceDuplication => {
            matches!(
                action_type,
                ActionType::ExtractFunction | ActionType::RemoveCode
            )
        }
        Goal::RemoveBug => action_type == ActionType::FixBug,
    }
}

fn primary_action_type(graph: &CausalGraph) -> Option<ActionType> {
    graph.actions.first().map(|action| action.action_type)
}

fn primary_action_order(graph: &CausalGraph) -> usize {
    primary_action_type(graph)
        .map(action_type_order)
        .unwrap_or(usize::MAX)
}

fn selection_action_priority(graph: &CausalGraph) -> usize {
    primary_action_type(graph)
        .map(selection_action_type_priority)
        .unwrap_or(usize::MAX)
}

fn selection_action_type_priority(action_type: ActionType) -> usize {
    match action_type {
        ActionType::FixBug => 0,
        ActionType::ExtractFunction => 1,
        ActionType::Rename => 2,
        ActionType::Inline => 3,
        ActionType::RemoveCode => 4,
    }
}

fn fallback_causal_graph() -> CausalGraph {
    initial_causal_graphs_from_nl("")
        .into_iter()
        .next()
        .expect("fallback graph must be generated")
}

fn action_candidates_for_causes(causes: &[Cause]) -> Vec<Action> {
    let mut action_types = Vec::new();
    for cause in causes {
        match cause {
            Cause::ReadabilityLow => {
                action_types.push(ActionType::ExtractFunction);
                action_types.push(ActionType::Inline);
                action_types.push(ActionType::Rename);
            }
            Cause::DuplicationHigh => {
                action_types.push(ActionType::ExtractFunction);
                action_types.push(ActionType::RemoveCode);
            }
            Cause::BugPresent => {
                action_types.push(ActionType::FixBug);
            }
        }
    }

    action_types.sort_by_key(|action_type| action_type_order(*action_type));
    action_types.dedup();
    action_types.into_iter().map(action_for_expansion).collect()
}

fn action_for_expansion(action_type: ActionType) -> Action {
    Action::with_confidence(action_type, confidence_for_action(action_type))
}

fn confidence_for_action(action_type: ActionType) -> f32 {
    match action_type {
        ActionType::ExtractFunction => 0.7,
        ActionType::Rename => 0.6,
        ActionType::Inline => 0.5,
        ActionType::FixBug => 0.9,
        ActionType::RemoveCode => 0.6,
    }
}

fn action_type_order(action_type: ActionType) -> usize {
    match action_type {
        ActionType::ExtractFunction => 0,
        ActionType::Inline => 1,
        ActionType::Rename => 2,
        ActionType::FixBug => 3,
        ActionType::RemoveCode => 4,
    }
}

fn infer_causes(intent_type: IntentType) -> Vec<Cause> {
    match intent_type {
        IntentType::Refactor | IntentType::Rename => vec![Cause::ReadabilityLow],
        IntentType::FixBug => vec![Cause::BugPresent],
    }
}

fn generate_goals(intent_type: IntentType) -> Vec<Goal> {
    match intent_type {
        IntentType::Refactor | IntentType::Rename => vec![Goal::ImproveReadability],
        IntentType::FixBug => vec![Goal::RemoveBug],
    }
}

fn generate_actions(intent_type: IntentType) -> Vec<Action> {
    match intent_type {
        IntentType::Refactor => vec![
            Action::new(ActionType::ExtractFunction),
            Action::new(ActionType::Inline),
            Action::new(ActionType::Rename),
        ],
        IntentType::FixBug => vec![Action::new(ActionType::FixBug)],
        IntentType::Rename => vec![Action::new(ActionType::Rename)],
    }
}

fn default_constraints() -> Vec<Constraint> {
    vec![Constraint::NoBehaviorChange, Constraint::ScopeLimited]
}

fn deterministic_intent_id(input: &str, index: usize) -> String {
    let mut hasher = DefaultHasher::new();
    input.hash(&mut hasher);
    index.hash(&mut hasher);
    hasher.finish().to_string()
}

impl Default for CausalValidation {
    fn default() -> Self {
        Self {
            valid: true,
            issues: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_file(name: &str, content: &str) -> PathBuf {
        let path =
            std::env::temp_dir().join(format!("causal_domain_{}_{}", std::process::id(), name));
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(backup_path(&path));
        std::fs::write(&path, content).expect("write test file");
        path
    }

    #[test]
    fn phase1_step1_refactor_from_clean_up_text() {
        let graphs = initial_causal_graphs_from_nl("きれいにして");

        assert_eq!(graphs.len(), 1);
        let graph = &graphs[0];
        assert_eq!(graph.intent_type, IntentType::Refactor);
        assert!(graph.goals.contains(&Goal::ImproveReadability));
        assert!(graph.causes.contains(&Cause::ReadabilityLow));
        assert!(
            graph
                .actions
                .iter()
                .any(|action| action.action_type == ActionType::ExtractFunction)
        );
        assert!(
            graph
                .actions
                .iter()
                .all(|action| action.target == Target::Unknown)
        );
        assert!(graph.is_ir_convertible_minimal());
    }

    #[test]
    fn phase1_step1_fix_bug_from_fix_text() {
        let graphs = CausalGraph::from_natural_language("バグを直して");

        let graph = &graphs[0];
        assert_eq!(graph.intent_type, IntentType::FixBug);
        assert!(graph.causes.contains(&Cause::BugPresent));
        assert!(graph.goals.contains(&Goal::RemoveBug));
        assert!(
            graph
                .actions
                .iter()
                .any(|action| action.action_type == ActionType::FixBug)
        );
        assert!(graph.constraints.contains(&Constraint::NoBehaviorChange));
        assert!(graph.constraints.contains(&Constraint::ScopeLimited));
    }

    #[test]
    fn phase1_step1_unknown_input_falls_back_to_refactor() {
        let graphs = initial_causal_graphs_from_nl("何とかして");

        assert_eq!(graphs.len(), 1);
        assert_eq!(graphs[0].intent_type, IntentType::Refactor);
        assert!(graphs[0].is_ir_convertible_minimal());
    }

    #[test]
    fn phase1_step1_is_deterministic_and_never_empty_for_blank_input() {
        let lhs = initial_causal_graphs_from_nl("");
        let rhs = initial_causal_graphs_from_nl("");

        assert_eq!(lhs, rhs);
        assert_eq!(lhs.len(), 1);
        assert!(lhs[0].is_ir_convertible_minimal());
    }

    #[test]
    fn phase1_step1_intent_id_uses_index_to_avoid_collisions() {
        assert_eq!(
            deterministic_intent_id("きれいにして", 0),
            deterministic_intent_id("きれいにして", 0)
        );
        assert_ne!(
            deterministic_intent_id("きれいにして", 0),
            deterministic_intent_id("きれいにして", 1)
        );
    }

    #[test]
    fn phase1_step1_generated_graph_passes_structural_validation() {
        let graph = initial_causal_graphs_from_nl("きれいにして")
            .into_iter()
            .next()
            .expect("graph");

        let validation = graph.validate();

        assert!(validation.valid, "{:?}", validation.issues);
    }

    #[test]
    fn phase1_step2_refactor_expands_by_readability_actions() {
        let graphs = expand_causal_graphs(initial_causal_graphs_from_nl("きれいにして"));
        let action_types: Vec<_> = graphs
            .iter()
            .map(|graph| graph.actions[0].action_type)
            .collect();

        assert_eq!(graphs.len(), 3);
        assert_eq!(
            action_types,
            vec![
                ActionType::ExtractFunction,
                ActionType::Inline,
                ActionType::Rename
            ]
        );
        assert!(graphs.iter().all(|graph| graph.validate().valid));
        assert!(graphs.iter().all(|graph| graph.actions.len() == 1));
        assert!(
            graphs
                .iter()
                .all(|graph| graph.actions[0].target == Target::Unknown)
        );
    }

    #[test]
    fn phase1_step2_fix_bug_expands_to_minimum_graph_count() {
        let graphs = CausalGraph::expand(initial_causal_graphs_from_nl("バグを直して"));

        assert_eq!(graphs.len(), MIN_GRAPHS);
        assert!(
            graphs
                .iter()
                .all(|graph| graph.actions[0].action_type == ActionType::FixBug)
        );
        assert!(
            graphs
                .iter()
                .all(|graph| (graph.actions[0].confidence - 0.9).abs() < f32::EPSILON)
        );
        assert_ne!(graphs[0].intent_id, graphs[1].intent_id);
        assert!(graphs.iter().all(|graph| graph.validate().valid));
    }

    #[test]
    fn phase1_step2_multiple_causes_are_limited_to_max_graphs() {
        let mut graph = initial_causal_graphs_from_nl("きれいにして")
            .into_iter()
            .next()
            .expect("graph");
        graph.causes = vec![
            Cause::ReadabilityLow,
            Cause::DuplicationHigh,
            Cause::BugPresent,
        ];

        let graphs = expand_causal_graphs(vec![graph]);
        let action_types: Vec<_> = graphs
            .iter()
            .map(|graph| graph.actions[0].action_type)
            .collect();

        assert!(graphs.len() <= MAX_GRAPHS);
        assert_eq!(
            action_types,
            vec![
                ActionType::ExtractFunction,
                ActionType::Inline,
                ActionType::Rename,
                ActionType::FixBug,
                ActionType::RemoveCode
            ]
        );
        assert!(graphs.iter().all(|graph| graph.validate().valid));
    }

    #[test]
    fn phase1_step2_expansion_is_deterministic() {
        let input = initial_causal_graphs_from_nl("きれいにして");

        let lhs = expand_causal_graphs(input.clone());
        let rhs = expand_causal_graphs(input);

        assert_eq!(lhs, rhs);
    }

    #[test]
    fn phase1_step3_refactor_scores_extract_over_rename_over_inline() {
        let expanded = expand_causal_graphs(initial_causal_graphs_from_nl("きれいにして"));
        let scored = score_causal_graphs(expanded);
        let action_types: Vec<_> = scored
            .iter()
            .map(|scored| scored.graph.actions[0].action_type)
            .collect();

        assert_eq!(
            action_types,
            vec![
                ActionType::ExtractFunction,
                ActionType::Rename,
                ActionType::Inline
            ]
        );
        assert!(scored[0].score > scored[1].score);
        assert!(scored[1].score > scored[2].score);
    }

    #[test]
    fn phase1_step3_fix_bug_scores_fix_bug_highest() {
        let mut graph = initial_causal_graphs_from_nl("バグを直して")
            .into_iter()
            .next()
            .expect("graph");
        graph.causes = vec![Cause::ReadabilityLow, Cause::BugPresent];
        graph.goals = vec![Goal::ImproveReadability, Goal::RemoveBug];

        let scored = CausalGraph::score(expand_causal_graphs(vec![graph]));

        assert_eq!(scored[0].graph.actions[0].action_type, ActionType::FixBug);
        assert!(scored[0].score >= scored[1].score);
    }

    #[test]
    fn phase1_step3_remove_code_gets_low_score_for_safety_violation() {
        let mut graph = initial_causal_graphs_from_nl("きれいにして")
            .into_iter()
            .next()
            .expect("graph");
        graph.causes = vec![Cause::DuplicationHigh];
        graph.goals = vec![Goal::ReduceDuplication];

        let scored = score_causal_graphs(expand_causal_graphs(vec![graph]));
        let remove_code = scored
            .iter()
            .find(|scored| scored.graph.actions[0].action_type == ActionType::RemoveCode)
            .expect("remove_code candidate");
        let extract_function = scored
            .iter()
            .find(|scored| scored.graph.actions[0].action_type == ActionType::ExtractFunction)
            .expect("extract_function candidate");

        assert!(remove_code.score < extract_function.score);
        assert!(remove_code.score < 0.5);
    }

    #[test]
    fn phase1_step3_scoring_order_is_deterministic() {
        let input = expand_causal_graphs(initial_causal_graphs_from_nl("きれいにして"));

        let lhs = score_causal_graphs(input.clone());
        let rhs = score_causal_graphs(input);

        assert_eq!(lhs, rhs);
    }

    #[test]
    fn phase1_step4_selects_highest_score_graph() {
        let scored = score_causal_graphs(expand_causal_graphs(initial_causal_graphs_from_nl(
            "きれいにして",
        )));

        let selected = select_best_graph(scored);

        assert_eq!(selected.actions[0].action_type, ActionType::ExtractFunction);
        assert!(selected.validate().valid);
    }

    #[test]
    fn phase1_step4_tie_breaks_by_selection_action_priority() {
        let base = initial_causal_graphs_from_nl("きれいにして")
            .into_iter()
            .next()
            .expect("graph");
        let mut inline = base.clone();
        inline.actions = vec![Action::new(ActionType::Inline)];
        let mut fix_bug = base.clone();
        fix_bug.actions = vec![Action::new(ActionType::FixBug)];
        let mut rename = base;
        rename.actions = vec![Action::new(ActionType::Rename)];

        let selected = select_best_graph(vec![
            ScoredGraph {
                graph: inline,
                score: 0.75,
            },
            ScoredGraph {
                graph: rename,
                score: 0.75,
            },
            ScoredGraph {
                graph: fix_bug,
                score: 0.75,
            },
        ]);

        assert_eq!(selected.actions[0].action_type, ActionType::FixBug);
    }

    #[test]
    fn phase1_step4_selection_is_deterministic() {
        let scored = score_causal_graphs(expand_causal_graphs(initial_causal_graphs_from_nl(
            "きれいにして",
        )));

        let lhs = CausalGraph::select(scored.clone());
        let rhs = CausalGraph::select(scored);

        assert_eq!(lhs, rhs);
    }

    #[test]
    fn phase1_step4_remove_code_is_not_selected_when_safer_graph_exists() {
        let mut graph = initial_causal_graphs_from_nl("きれいにして")
            .into_iter()
            .next()
            .expect("graph");
        graph.causes = vec![Cause::DuplicationHigh];
        graph.goals = vec![Goal::ReduceDuplication];

        let selected = select_best_graph(score_causal_graphs(expand_causal_graphs(vec![graph])));

        assert_ne!(selected.actions[0].action_type, ActionType::RemoveCode);
        assert_eq!(selected.actions[0].action_type, ActionType::ExtractFunction);
    }

    #[test]
    fn phase1_step4_empty_input_falls_back_to_valid_graph() {
        let selected = select_best_graph(Vec::new());

        assert!(selected.validate().valid);
        assert!(selected.is_ir_convertible_minimal());
    }

    #[test]
    fn phase2_extract_function_maps_to_refactor_extract_function_ir() {
        let graph = select_best_graph(score_causal_graphs(expand_causal_graphs(
            initial_causal_graphs_from_nl("きれいにして"),
        )));

        let ir = causal_graph_to_ir(&graph);

        assert_eq!(ir.len(), 1);
        assert_eq!(ir[0].op, IrOp::RefactorExtractFunction);
        assert_eq!(ir[0].target, Target::Unknown);
    }

    #[test]
    fn phase2_fix_bug_maps_to_fix_bug_ir() {
        let graph = select_best_graph(score_causal_graphs(expand_causal_graphs(
            initial_causal_graphs_from_nl("バグを直して"),
        )));

        let ir = CausalGraph::to_ir(&graph);

        assert_eq!(ir.len(), 1);
        assert_eq!(ir[0].op, IrOp::FixBug);
        assert_eq!(ir[0].target, Target::Unknown);
    }

    #[test]
    fn phase2_remove_code_maps_to_remove_code_ir() {
        let mut graph = initial_causal_graphs_from_nl("きれいにして")
            .into_iter()
            .next()
            .expect("graph");
        graph.causes = vec![Cause::DuplicationHigh];
        graph.goals = vec![Goal::ReduceDuplication];
        graph.actions = vec![Action::with_confidence(ActionType::RemoveCode, 0.6)];

        let ir = causal_graph_to_ir(&graph);

        assert_eq!(ir.len(), 1);
        assert_eq!(ir[0].op, IrOp::RemoveCode);
    }

    #[test]
    fn phase2_ir_conversion_is_deterministic_and_copies_target_and_params() {
        let mut graph = initial_causal_graphs_from_nl("名前変更して")
            .into_iter()
            .next()
            .expect("graph");
        let mut action = Action::new(ActionType::Rename);
        action.target = Target::Symbol("old_name".to_string());
        action
            .params
            .insert("new_name".to_string(), "new_name".to_string());
        graph.actions = vec![action];

        let lhs = causal_graph_to_ir(&graph);
        let rhs = causal_graph_to_ir(&graph);

        assert_eq!(lhs, rhs);
        assert_eq!(lhs[0].op, IrOp::RefactorRename);
        assert_eq!(lhs[0].target, Target::Symbol("old_name".to_string()));
        assert_eq!(
            lhs[0].params.get("new_name").map(String::as_str),
            Some("new_name")
        );
    }

    #[test]
    fn phase2_invalid_graph_falls_back_to_extract_function_ir() {
        let graph = CausalGraph::new();

        let ir = causal_graph_to_ir(&graph);

        assert_eq!(ir.len(), 1);
        assert_eq!(ir[0].op, IrOp::RefactorExtractFunction);
        assert_eq!(ir[0].target, Target::Unknown);
        assert!(ir[0].params.is_empty());
    }

    #[test]
    fn phase3_preview_generates_diff_without_apply() {
        let ir = vec![IrStep {
            op: IrOp::RefactorExtractFunction,
            target: Target::Unknown,
            params: HashMap::new(),
        }];
        let expected_changes = vec![Change::Add {
            path: "ir/unknown/refactor_extract_function.plan".to_string(),
            content: "refactor.extract_function".to_string(),
        }];

        let result = execute_ir_steps(ir);

        assert_eq!(result.status, ExecutionStatus::PreviewOnly);
        assert!(!result.applied);
        assert_eq!(
            result.diff,
            Some(Diff {
                checksum: checksum_changes(&expected_changes),
                changes: expected_changes,
            })
        );
    }

    #[test]
    fn phase3_remove_code_validation_rejects_diff() {
        let ir = vec![IrStep {
            op: IrOp::RemoveCode,
            target: Target::Unknown,
            params: HashMap::new(),
        }];
        let expected_changes = vec![Change::Remove {
            path: "ir/unknown/remove_code.plan".to_string(),
        }];

        let result = execute_ir_steps(ir);

        assert_eq!(result.status, ExecutionStatus::Rejected);
        assert!(!result.applied);
        assert_eq!(
            result.diff,
            Some(Diff {
                checksum: checksum_changes(&expected_changes),
                changes: expected_changes,
            })
        );
    }

    #[test]
    fn phase3_apply_success_marks_result_applied() {
        let file = test_file("phase3_apply_success.rs", "fn buggy_fn() {}\n");
        let ir = vec![IrStep {
            op: IrOp::FixBug,
            target: Target::Symbol(file.to_string_lossy().to_string()),
            params: HashMap::new(),
        }];

        let result = execute_ir_steps_with_options(
            ir,
            ExecutionOptions {
                preview_only: false,
                apply: true,
                simulate_apply_failure: false,
                ..ExecutionOptions::default()
            },
        );

        assert_eq!(result.status, ExecutionStatus::Applied);
        assert!(result.applied);
        assert_eq!(
            std::fs::read_to_string(&file).unwrap(),
            "// dbm: fix bug preview\n"
        );
    }

    #[test]
    fn phase3_apply_failure_rolls_back_and_reports_failed() {
        let file = test_file("phase3_apply_failure.rs", "fn name() {}\n");
        let ir = vec![IrStep {
            op: IrOp::RefactorRename,
            target: Target::Symbol(file.to_string_lossy().to_string()),
            params: HashMap::new(),
        }];

        let result = execute_ir_steps_with_options(
            ir,
            ExecutionOptions {
                preview_only: false,
                apply: true,
                simulate_apply_failure: true,
                ..ExecutionOptions::default()
            },
        );

        assert_eq!(result.status, ExecutionStatus::Failed);
        assert!(!result.applied);
        assert!(result.diff.is_some());
        assert_eq!(std::fs::read_to_string(file).unwrap(), "fn name() {}\n");
    }

    #[test]
    fn phase3_execution_is_deterministic_for_same_ir() {
        let ir = vec![IrStep {
            op: IrOp::RefactorInline,
            target: Target::Unknown,
            params: HashMap::new(),
        }];

        let lhs = execute_ir_steps(ir.clone());
        let rhs = execute_ir_steps(ir);

        assert_eq!(lhs, rhs);
    }

    #[test]
    fn phase4_target_resolution_maps_existing_symbol_to_file() {
        let file = test_file("phase4_resolve.rs", "fn main() {}\n");

        let resolved = resolve_target(&Target::Symbol(file.to_string_lossy().to_string()));

        assert_eq!(resolved, ResolvedTarget::File(file));
        assert_eq!(
            resolve_target(&Target::Unknown),
            ResolvedTarget::SymbolUnknown
        );
    }

    #[test]
    fn phase4_normal_apply_updates_file_and_creates_backup() {
        let file = test_file("phase4_apply.rs", "fn main() {}\n");
        let ir = vec![IrStep {
            op: IrOp::RefactorExtractFunction,
            target: Target::Symbol(file.to_string_lossy().to_string()),
            params: HashMap::new(),
        }];
        let expected_changes = vec![Change::InsertLine {
            file: file.clone(),
            line: 2,
            content: "// dbm: extract function placeholder".to_string(),
        }];

        let result = execute_ir_steps_with_options(
            ir,
            ExecutionOptions {
                preview_only: false,
                apply: true,
                ..ExecutionOptions::default()
            },
        );

        assert_eq!(result.status, ExecutionStatus::Applied);
        assert!(result.applied);
        assert_eq!(
            result.diff,
            Some(Diff {
                checksum: checksum_changes(&expected_changes),
                changes: expected_changes,
            })
        );
        assert_eq!(
            std::fs::read_to_string(&file).unwrap(),
            "fn main() {}\n// dbm: extract function placeholder\n"
        );
        assert!(backup_path(&file).exists());
    }

    #[test]
    fn phase4_unknown_target_cannot_apply_to_files() {
        let ir = vec![IrStep {
            op: IrOp::FixBug,
            target: Target::Unknown,
            params: HashMap::new(),
        }];

        let result = execute_ir_steps_with_options(
            ir,
            ExecutionOptions {
                preview_only: false,
                apply: true,
                ..ExecutionOptions::default()
            },
        );

        assert_eq!(result.status, ExecutionStatus::Rejected);
        assert!(!result.applied);
    }

    #[test]
    fn phase4_remove_code_is_rejected_without_destructive_permission() {
        let file = test_file("phase4_remove.rs", "fn dead() {}\n");
        let ir = vec![IrStep {
            op: IrOp::RemoveCode,
            target: Target::Symbol(file.to_string_lossy().to_string()),
            params: HashMap::new(),
        }];

        let result = execute_ir_steps_with_options(
            ir,
            ExecutionOptions {
                preview_only: false,
                apply: true,
                allow_destructive: false,
                ..ExecutionOptions::default()
            },
        );

        assert_eq!(result.status, ExecutionStatus::Rejected);
        assert_eq!(std::fs::read_to_string(file).unwrap(), "fn dead() {}\n");
    }

    #[test]
    fn phase4_apply_failure_restores_original_file() {
        let file = test_file("phase4_rollback.rs", "fn keep() {}\n");
        let ir = vec![IrStep {
            op: IrOp::RefactorInline,
            target: Target::Symbol(file.to_string_lossy().to_string()),
            params: HashMap::new(),
        }];

        let result = execute_ir_steps_with_options(
            ir,
            ExecutionOptions {
                preview_only: false,
                apply: true,
                simulate_apply_failure: true,
                ..ExecutionOptions::default()
            },
        );

        assert_eq!(result.status, ExecutionStatus::Failed);
        assert_eq!(std::fs::read_to_string(file).unwrap(), "fn keep() {}\n");
    }

    #[test]
    fn phase4_same_ir_produces_same_file_diff() {
        let file = test_file("phase4_determinism.rs", "fn stable() {}\n");
        let ir = vec![IrStep {
            op: IrOp::RefactorExtractFunction,
            target: Target::Symbol(file.to_string_lossy().to_string()),
            params: HashMap::new(),
        }];

        let lhs = execute_ir_steps(ir.clone());
        let rhs = execute_ir_steps(ir);

        assert_eq!(lhs, rhs);
    }

    #[test]
    fn phase4_hardening_force_dry_run_overrides_apply() {
        let file = test_file("phase4_force_dry_run.rs", "fn stable() {}\n");
        let ir = vec![IrStep {
            op: IrOp::RefactorExtractFunction,
            target: Target::Symbol(file.to_string_lossy().to_string()),
            params: HashMap::new(),
        }];

        let result = execute_ir_steps_with_options(
            ir,
            ExecutionOptions {
                preview_only: false,
                apply: true,
                force_dry_run: true,
                ..ExecutionOptions::default()
            },
        );

        assert_eq!(result.status, ExecutionStatus::PreviewOnly);
        assert!(!result.applied);
        assert_eq!(std::fs::read_to_string(file).unwrap(), "fn stable() {}\n");
    }

    #[test]
    fn phase4_hardening_checksum_is_deterministic() {
        let changes = vec![
            Change::Add {
                path: "a".to_string(),
                content: "one".to_string(),
            },
            Change::Modify {
                path: "b".to_string(),
                before: "two".to_string(),
                after: "three".to_string(),
            },
        ];

        assert_eq!(checksum_changes(&changes), checksum_changes(&changes));
        assert_ne!(checksum_changes(&changes), 0);
    }

    #[test]
    fn phase4_hardening_checksum_order_is_part_of_hash() {
        let lhs = vec![
            Change::Add {
                path: "a".to_string(),
                content: "one".to_string(),
            },
            Change::Remove {
                path: "b".to_string(),
            },
        ];
        let rhs = vec![
            Change::Remove {
                path: "b".to_string(),
            },
            Change::Add {
                path: "a".to_string(),
                content: "one".to_string(),
            },
        ];

        assert_ne!(checksum_changes(&lhs), checksum_changes(&rhs));
    }

    #[test]
    fn phase4_hardening_checksum_mismatch_is_rejected() {
        let file = test_file("phase4_checksum_mismatch.rs", "fn stable() {}\n");
        let changes = vec![Change::InsertLine {
            file: file.clone(),
            line: 2,
            content: "// dbm: extract function placeholder".to_string(),
        }];
        let diff = Diff {
            changes,
            checksum: 1,
        };
        let resolved_targets = vec![ResolvedTarget::File(file)];

        assert!(!validate_diff(
            &diff,
            &resolved_targets,
            &ExecutionOptions {
                preview_only: false,
                apply: true,
                ..ExecutionOptions::default()
            }
        ));
    }

    #[test]
    fn phase4_hardening_force_dry_run_has_priority_over_apply() {
        let file = test_file("phase4_force_priority.rs", "fn stable() {}\n");
        let ir = vec![IrStep {
            op: IrOp::FixBug,
            target: Target::Symbol(file.to_string_lossy().to_string()),
            params: HashMap::new(),
        }];

        let result = execute_ir_steps_with_options(
            ir,
            ExecutionOptions {
                preview_only: false,
                apply: true,
                force_dry_run: true,
                ..ExecutionOptions::default()
            },
        );

        assert_eq!(result.status, ExecutionStatus::PreviewOnly);
        assert_eq!(std::fs::read_to_string(file).unwrap(), "fn stable() {}\n");
    }

    #[test]
    fn computes_transitive_closure() {
        let mut graph = CausalGraph::new();
        graph.add_edge(1, 2, CausalRelationKind::Requires);
        graph.add_edge(2, 3, CausalRelationKind::Enables);

        let closure = graph.causal_closure(1);

        assert!(closure.contains(&2));
        assert!(closure.contains(&3));
    }

    #[test]
    fn rejects_cycles() {
        let mut graph = CausalGraph::new();
        graph.add_edge(1, 2, CausalRelationKind::Requires);
        graph.add_edge(2, 1, CausalRelationKind::Requires);

        let validation = graph.validate();

        assert!(!validation.valid);
        assert!(
            validation
                .issues
                .iter()
                .any(|issue| issue.contains("causal cycle"))
        );
    }
}

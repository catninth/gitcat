use std::{
    collections::{BTreeMap, HashMap, HashSet},
    env, fs,
    io::Write,
    path::{Component, Path, PathBuf},
    process::{Command, Stdio},
    time::{SystemTime, UNIX_EPOCH},
};

use gitcat_contracts::{CommitSummary, HeadState, HistoryQuery, HistoryScope};
use gitcat_core::GitBackend;
use gitcat_git_cli::GitCliBackend;
use serde::Deserialize;

const CHECKOUT_SWITCH: &str =
    include_str!("fixtures/graph-conformance/scenarios/checkout-switch.json");
const CHECKOUT_SWITCH_ORACLE: &str = include_str!(
    "fixtures/graph-conformance/oracles/gitkraken-12.4.0-windows/checkout-switch.json"
);
const CHECKOUT_WIP: &str = include_str!("fixtures/graph-conformance/scenarios/checkout-wip.json");
const CHECKOUT_WIP_ORACLE: &str =
    include_str!("fixtures/graph-conformance/oracles/gitkraken-12.4.0-windows/checkout-wip.json");
const STASH_SINGLE: &str = include_str!("fixtures/graph-conformance/scenarios/stash-single.json");
const STASH_SINGLE_ORACLE: &str =
    include_str!("fixtures/graph-conformance/oracles/gitkraken-12.4.0-windows/stash-single.json");
const STASH_LIFECYCLE: &str =
    include_str!("fixtures/graph-conformance/scenarios/stash-lifecycle.json");
const STASH_LIFECYCLE_ORACLE: &str = include_str!(
    "fixtures/graph-conformance/oracles/gitkraken-12.4.0-windows/stash-lifecycle.json"
);
const STASH_CHECKOUT: &str =
    include_str!("fixtures/graph-conformance/scenarios/stash-checkout.json");
const STASH_CHECKOUT_ORACLE: &str =
    include_str!("fixtures/graph-conformance/oracles/gitkraken-12.4.0-windows/stash-checkout.json");
const DISCONNECTED_CHECKOUT: &str =
    include_str!("fixtures/graph-conformance/scenarios/disconnected-checkout.json");
const DISCONNECTED_CHECKOUT_ORACLE: &str = include_str!(
    "fixtures/graph-conformance/oracles/gitkraken-12.4.0-windows/disconnected-checkout.json"
);
const DISCONNECTED_INTERIOR_REUSE: &str =
    include_str!("fixtures/graph-conformance/scenarios/disconnected-interior-reuse.json");
const DISCONNECTED_INTERIOR_REUSE_ORACLE: &str = include_str!(
    "fixtures/graph-conformance/oracles/gitkraken-12.4.0-windows/disconnected-interior-reuse.json"
);
const STASH_INDEX_COLLISION: &str =
    include_str!("fixtures/graph-conformance/scenarios/stash-index-collision.json");
const STASH_INDEX_COLLISION_ORACLE: &str = include_str!(
    "fixtures/graph-conformance/oracles/gitkraken-12.4.0-windows/stash-index-collision.json"
);
const MERGE_LADDER: &str = include_str!("fixtures/graph-conformance/scenarios/merge-ladder.json");
const MERGE_LADDER_ORACLE: &str =
    include_str!("fixtures/graph-conformance/oracles/gitkraken-12.4.0-windows/merge-ladder.json");
const CONVERGENCE_LANE_REUSE: &str =
    include_str!("fixtures/graph-conformance/scenarios/convergence-lane-reuse.json");
const CONVERGENCE_LANE_REUSE_ORACLE: &str = include_str!(
    "fixtures/graph-conformance/oracles/gitkraken-12.4.0-windows/convergence-lane-reuse.json"
);

#[derive(Debug, Deserialize)]
struct Scenario {
    schema_version: u32,
    id: String,
    description: String,
    initial_branch: String,
    commits: Vec<ScenarioCommit>,
    refs: Vec<ScenarioRef>,
    #[serde(default)]
    worktree: Vec<ScenarioWorktreeFile>,
    #[serde(default)]
    initial_stashes: Vec<ScenarioInitialStash>,
    steps: Vec<ScenarioStep>,
}

#[derive(Debug, Deserialize)]
struct ScenarioCommit {
    id: String,
    parents: Vec<String>,
    subject: String,
    date: String,
    #[serde(default)]
    files: Vec<ScenarioTreeFile>,
}

#[derive(Debug, Deserialize)]
struct ScenarioTreeFile {
    path: String,
    contents: String,
}

#[derive(Debug, Deserialize)]
struct ScenarioRef {
    name: String,
    target: String,
}

#[derive(Debug, Deserialize)]
struct ScenarioWorktreeFile {
    path: String,
    contents: String,
}

#[derive(Debug, Deserialize)]
struct ScenarioInitialStash {
    message: String,
    date: String,
    changes: Vec<ScenarioWorktreeFile>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum ScenarioStep {
    ApplyStash { message: String },
    Checkpoint { id: String },
    Checkout { branch: String },
    CreateStash { message: String },
    DeleteStash { message: String },
    PopStash { message: String },
    ReopenRepository,
    RestartApplication,
}

#[derive(Debug, Deserialize)]
struct SemanticOracle {
    checkpoints: Vec<OracleCheckpoint>,
}

#[derive(Debug, Clone, Deserialize)]
struct OracleCheckpoint {
    id: String,
    head: String,
    #[serde(default)]
    rows: Option<Vec<OracleRow>>,
    #[serde(default)]
    same_graph_as: Option<String>,
    #[serde(default)]
    same_commits_as: Option<String>,
    #[serde(default)]
    wip: Option<OracleWip>,
    #[serde(default)]
    stashes: Option<Vec<OracleStash>>,
    #[serde(default)]
    worktree_files: Option<Vec<OracleWorktreeFile>>,
    #[serde(default)]
    raw_stashes: Option<Vec<OracleRawStash>>,
}

#[derive(Debug, Clone, Deserialize)]
struct OracleWorktreeFile {
    path: String,
    contents: String,
}

#[derive(Debug, Clone, Deserialize)]
struct OracleRawStash {
    selector: String,
    message: String,
}

#[derive(Debug, Clone, Deserialize)]
struct OracleRow {
    commit: String,
    row: usize,
    lane: usize,
    edges: Vec<OracleEdge>,
}

#[derive(Debug, Clone, Deserialize)]
struct OracleEdge {
    parent: String,
    parent_index: usize,
    from_lane: usize,
    to_lane: usize,
}

#[derive(Debug, Clone, Deserialize)]
struct OracleWip {
    head: String,
    lane: usize,
}

#[derive(Debug, Clone, Deserialize)]
struct OracleStash {
    message: String,
    selector: String,
    display_row: usize,
    lane: usize,
    parent: String,
    visible_commit_role: OracleStashVisibleCommitRole,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
enum OracleStashVisibleCommitRole {
    IndexParent,
}

#[derive(Debug, Clone)]
struct ResolvedCheckpoint {
    rows: Vec<OracleRow>,
    wip: Option<OracleWip>,
    stashes: Vec<OracleStash>,
}

#[derive(Debug)]
struct MaterializedScenario {
    repository: PathBuf,
    oids: BTreeMap<String, String>,
}

/// Every scenario gets its own oracle comparison, so the scenarios replay in
/// parallel and one mismatch does not hide the others, plus an ignored helper
/// that materializes the repository for a fresh GitKraken capture.
macro_rules! conformance_scenarios {
    ($($scenario:ident, $oracle:ident => $compare:ident, $materialize:ident;)+) => {
        const ALL_SCENARIOS: &[(&str, &str)] = &[$(($scenario, $oracle)),+];

        $(
            #[tokio::test]
            async fn $compare() {
                compare_with_oracle($scenario, $oracle).await;
            }

            #[test]
            #[ignore = "manually materializes a repository for GitKraken reference capture"]
            fn $materialize() {
                materialize_for_gitkraken($scenario);
            }
        )+
    };
}

conformance_scenarios! {
    CHECKOUT_SWITCH, CHECKOUT_SWITCH_ORACLE
        => oracle_checkout_switch, materialize_checkout_switch_for_gitkraken;
    CHECKOUT_WIP, CHECKOUT_WIP_ORACLE
        => oracle_checkout_wip, materialize_checkout_wip_for_gitkraken;
    STASH_SINGLE, STASH_SINGLE_ORACLE
        => oracle_stash_single, materialize_stash_single_for_gitkraken;
    STASH_LIFECYCLE, STASH_LIFECYCLE_ORACLE
        => oracle_stash_lifecycle, materialize_stash_lifecycle_for_gitkraken;
    STASH_CHECKOUT, STASH_CHECKOUT_ORACLE
        => oracle_stash_checkout, materialize_stash_checkout_for_gitkraken;
    DISCONNECTED_CHECKOUT, DISCONNECTED_CHECKOUT_ORACLE
        => oracle_disconnected_checkout, materialize_disconnected_checkout_for_gitkraken;
    DISCONNECTED_INTERIOR_REUSE, DISCONNECTED_INTERIOR_REUSE_ORACLE
        => oracle_disconnected_interior_reuse, materialize_disconnected_interior_reuse_for_gitkraken;
    STASH_INDEX_COLLISION, STASH_INDEX_COLLISION_ORACLE
        => oracle_stash_index_collision, materialize_stash_index_collision_for_gitkraken;
    CONVERGENCE_LANE_REUSE, CONVERGENCE_LANE_REUSE_ORACLE
        => oracle_convergence_lane_reuse, materialize_convergence_lane_reuse_for_gitkraken;
    MERGE_LADDER, MERGE_LADDER_ORACLE
        => oracle_merge_ladder, materialize_merge_ladder_for_gitkraken;
}

#[test]
fn graph_conformance_scenarios_are_valid() {
    for (scenario_json, oracle_json) in ALL_SCENARIOS {
        let scenario = parse_scenario(scenario_json);
        validate_scenario(&scenario);
        let oracle: serde_json::Value =
            serde_json::from_str(oracle_json).expect("GitKraken oracle is valid JSON");
        validate_oracle(&scenario, &oracle);
    }
}

async fn compare_with_oracle(scenario_json: &str, oracle_json: &str) {
    let scenario = parse_scenario(scenario_json);
    validate_scenario(&scenario);
    let oracle: SemanticOracle =
        serde_json::from_str(oracle_json).expect("GitKraken semantic oracle is valid JSON");
    let temporary = tempfile::tempdir().expect("temporary graph conformance directory");
    let materialized = materialize_scenario_in(&scenario, temporary.path());

    replay_and_compare(&scenario, &oracle, &materialized).await;
}

fn materialize_for_gitkraken(scenario_json: &str) {
    let scenario = parse_scenario(scenario_json);
    validate_scenario(&scenario);

    let repository = materialize_scenario(&scenario);
    let instructions = repository
        .parent()
        .expect("materialized repository has a parent")
        .join("capture-instructions.txt");
    fs::write(&instructions, capture_instructions(&scenario, &repository))
        .expect("write capture instructions");

    println!("GitKraken fixture: {}", repository.display());
    println!("Capture checklist: {}", instructions.display());
}

fn parse_scenario(json: &str) -> Scenario {
    serde_json::from_str(json).expect("graph conformance scenario is valid JSON")
}

fn validate_scenario(scenario: &Scenario) {
    assert_eq!(scenario.schema_version, 1, "unsupported scenario schema");
    assert!(!scenario.id.trim().is_empty(), "scenario id is required");
    assert!(
        !scenario.description.trim().is_empty(),
        "scenario description is required"
    );

    let mut commits = HashSet::new();
    for commit in &scenario.commits {
        assert!(
            commits.insert(commit.id.as_str()),
            "duplicate commit id: {}",
            commit.id
        );
        assert!(
            !commit.subject.trim().is_empty(),
            "commit subject is required"
        );
        assert!(!commit.date.trim().is_empty(), "commit date is required");
        let mut tree_paths = HashSet::new();
        for file in &commit.files {
            let path = Path::new(&file.path);
            assert!(
                !file.path.trim().is_empty()
                    && !path.is_absolute()
                    && path.components().count() == 1
                    && path
                        .components()
                        .all(|component| matches!(component, Component::Normal(_))),
                "tree fixture currently supports a root-level file: {}",
                file.path
            );
            assert!(
                tree_paths.insert(file.path.as_str()),
                "duplicate tree path in commit {}: {}",
                commit.id,
                file.path
            );
        }
        for parent in &commit.parents {
            assert!(
                commits.contains(parent.as_str()),
                "parent {parent} must appear before child {}",
                commit.id
            );
        }
    }

    let mut refs = HashSet::new();
    for reference in &scenario.refs {
        assert!(
            reference.name.starts_with("refs/"),
            "reference must use a full name: {}",
            reference.name
        );
        assert!(
            refs.insert(reference.name.as_str()),
            "duplicate reference: {}",
            reference.name
        );
        assert!(
            commits.contains(reference.target.as_str()),
            "unknown ref target: {}",
            reference.target
        );
    }

    let initial_ref = format!("refs/heads/{}", scenario.initial_branch);
    assert!(
        refs.contains(initial_ref.as_str()),
        "initial branch has no ref"
    );

    let mut worktree_paths = HashSet::new();
    for file in &scenario.worktree {
        let path = Path::new(&file.path);
        assert!(
            !file.path.trim().is_empty()
                && !path.is_absolute()
                && path
                    .components()
                    .all(|component| matches!(component, Component::Normal(_))),
            "worktree path must be a safe relative path: {}",
            file.path
        );
        assert!(
            worktree_paths.insert(file.path.as_str()),
            "duplicate worktree path: {}",
            file.path
        );
    }

    let mut stash_messages = HashSet::new();
    for stash in &scenario.initial_stashes {
        assert!(
            !stash.message.trim().is_empty(),
            "stash message is required"
        );
        assert!(!stash.date.trim().is_empty(), "stash date is required");
        assert!(
            stash_messages.insert(stash.message.as_str()),
            "duplicate initial stash message: {}",
            stash.message
        );
        assert!(!stash.changes.is_empty(), "initial stash needs a change");
        let mut stash_paths = HashSet::new();
        for file in &stash.changes {
            let path = Path::new(&file.path);
            assert!(
                !file.path.trim().is_empty()
                    && !path.is_absolute()
                    && path
                        .components()
                        .all(|component| matches!(component, Component::Normal(_))),
                "stash change path must be a safe relative path: {}",
                file.path
            );
            assert!(
                stash_paths.insert(file.path.as_str()),
                "duplicate path in initial stash {}: {}",
                stash.message,
                file.path
            );
        }
    }

    let mut checkpoints = HashSet::new();
    for step in &scenario.steps {
        match step {
            ScenarioStep::Checkpoint { id } => {
                assert!(
                    checkpoints.insert(id.as_str()),
                    "duplicate checkpoint: {id}"
                );
            }
            ScenarioStep::Checkout { branch } => {
                let branch_ref = format!("refs/heads/{branch}");
                assert!(
                    refs.contains(branch_ref.as_str()),
                    "unknown branch: {branch}"
                );
            }
            ScenarioStep::ApplyStash { message }
            | ScenarioStep::CreateStash { message }
            | ScenarioStep::DeleteStash { message }
            | ScenarioStep::PopStash { message } => {
                assert!(!message.trim().is_empty(), "stash message is required");
            }
            ScenarioStep::ReopenRepository | ScenarioStep::RestartApplication => {}
        }
    }
    assert!(
        !checkpoints.is_empty(),
        "scenario needs at least one checkpoint"
    );
}

fn validate_oracle(scenario: &Scenario, oracle: &serde_json::Value) {
    assert_eq!(oracle["schema_version"], 1, "unsupported oracle schema");
    assert_eq!(oracle["scenario"], scenario.id, "oracle scenario mismatch");
    assert_eq!(oracle["reference"]["product"], "GitKraken Desktop");
    assert_eq!(oracle["reference"]["version"], "12.4.0");
    assert_eq!(oracle["reference"]["platform"], "windows");
    assert!(
        oracle["observed_on"]
            .as_str()
            .is_some_and(|date| !date.is_empty()),
        "oracle observation date is required"
    );

    let scenario_checkpoints = scenario
        .steps
        .iter()
        .filter_map(|step| match step {
            ScenarioStep::Checkpoint { id } => Some(id.as_str()),
            _ => None,
        })
        .collect::<HashSet<_>>();
    let oracle_checkpoints = oracle["checkpoints"]
        .as_array()
        .expect("oracle checkpoints are an array");
    assert_eq!(oracle_checkpoints.len(), scenario_checkpoints.len());

    for checkpoint in oracle_checkpoints {
        let id = checkpoint["id"]
            .as_str()
            .expect("checkpoint id is a string");
        assert!(
            scenario_checkpoints.contains(id),
            "unknown oracle checkpoint: {id}"
        );
        let head = checkpoint["head"]
            .as_str()
            .expect("checkpoint head is a string");
        assert!(
            scenario
                .refs
                .iter()
                .any(|reference| { reference.name == format!("refs/heads/{head}") }),
            "unknown checkpoint head: {head}"
        );

        if checkpoint.get("same_graph_as").is_some() || checkpoint.get("same_commits_as").is_some()
        {
            assert!(
                checkpoint.get("rows").is_none(),
                "aliased checkpoint repeats rows"
            );
            continue;
        }

        let rows = checkpoint["rows"]
            .as_array()
            .expect("checkpoint rows are an array");
        assert_eq!(rows.len(), scenario.commits.len());
        let row_commits = rows
            .iter()
            .map(|row| row["commit"].as_str().expect("row commit is a string"))
            .collect::<HashSet<_>>();
        assert_eq!(row_commits.len(), scenario.commits.len());
        for commit in &scenario.commits {
            assert!(row_commits.contains(commit.id.as_str()));
        }
    }

    for checkpoint in oracle_checkpoints {
        for alias in ["same_graph_as", "same_commits_as"] {
            let Some(target) = checkpoint.get(alias) else {
                continue;
            };
            let target = target.as_str().expect("same_graph_as is a string");
            assert!(
                oracle_checkpoints
                    .iter()
                    .any(|candidate| candidate["id"] == target),
                "unknown graph alias target: {target}"
            );
        }
    }
}

async fn replay_and_compare(
    scenario: &Scenario,
    oracle: &SemanticOracle,
    materialized: &MaterializedScenario,
) {
    let backend = GitCliBackend::default();
    let checkpoints = oracle
        .checkpoints
        .iter()
        .map(|checkpoint| (checkpoint.id.as_str(), checkpoint))
        .collect::<HashMap<_, _>>();

    for step in &scenario.steps {
        match step {
            ScenarioStep::ApplyStash { message } => {
                let oid = visible_stash_oid_by_message(
                    &backend,
                    &materialized.repository,
                    message,
                    &scenario.id,
                )
                .await;
                backend
                    .stash_apply(&materialized.repository, &oid, false)
                    .await
                    .unwrap_or_else(|error| {
                        panic!("{}: apply stash {message:?} failed: {error}", scenario.id)
                    });
            }
            ScenarioStep::Checkpoint { id } => {
                let checkpoint = checkpoints
                    .get(id.as_str())
                    .unwrap_or_else(|| panic!("{}: oracle has no checkpoint {id}", scenario.id));
                let resolved = resolve_checkpoint(&checkpoints, id, &mut Vec::new());
                compare_checkpoint(scenario, checkpoint, &resolved, materialized, &backend).await;
            }
            ScenarioStep::Checkout { branch } => {
                backend
                    .checkout_branch(&materialized.repository, branch)
                    .await
                    .unwrap_or_else(|error| {
                        panic!("{}: checkout {branch:?} failed: {error}", scenario.id)
                    });
            }
            ScenarioStep::CreateStash { message } => {
                backend
                    .stash_push(&materialized.repository, Some(message), true)
                    .await
                    .unwrap_or_else(|error| {
                        panic!("{}: create stash {message:?} failed: {error}", scenario.id)
                    });
            }
            ScenarioStep::DeleteStash { message } => {
                let oid = visible_stash_oid_by_message(
                    &backend,
                    &materialized.repository,
                    message,
                    &scenario.id,
                )
                .await;
                backend
                    .stash_drop(&materialized.repository, &oid, true)
                    .await
                    .unwrap_or_else(|error| {
                        panic!("{}: delete stash {message:?} failed: {error}", scenario.id)
                    });
            }
            ScenarioStep::PopStash { message } => {
                let oid = visible_stash_oid_by_message(
                    &backend,
                    &materialized.repository,
                    message,
                    &scenario.id,
                )
                .await;
                backend
                    .stash_apply(&materialized.repository, &oid, true)
                    .await
                    .unwrap_or_else(|error| {
                        panic!("{}: pop stash {message:?} failed: {error}", scenario.id)
                    });
            }
            ScenarioStep::ReopenRepository | ScenarioStep::RestartApplication => {}
        }
    }
}

async fn visible_stash_oid_by_message(
    backend: &GitCliBackend,
    repository: &Path,
    message: &str,
    scenario_id: &str,
) -> String {
    let history = backend
        .history(
            repository,
            &HistoryQuery {
                scope: HistoryScope::AllRefs,
                cursor: None,
                limit: 200,
            },
        )
        .await
        .unwrap_or_else(|error| panic!("{scenario_id}: list visible stashes failed: {error}"));
    let matches = history
        .commits
        .iter()
        .filter(|commit| commit.stash.is_some() && stash_message_matches(&commit.subject, message))
        .collect::<Vec<_>>();
    assert_eq!(
        matches.len(),
        1,
        "{scenario_id}: expected exactly one visible stash matching {message:?}; available messages: {:?}",
        history
            .commits
            .iter()
            .filter(|commit| commit.stash.is_some())
            .map(|commit| commit.subject.as_str())
            .collect::<Vec<_>>()
    );
    matches[0]
        .stash
        .as_ref()
        .expect("matched visible stash has action metadata")
        .oid
        .clone()
}

fn stash_message_matches(actual: &str, expected: &str) -> bool {
    actual == expected
        || actual
            .strip_prefix("On ")
            .or_else(|| actual.strip_prefix("WIP on "))
            .and_then(|rest| rest.split_once(": "))
            .is_some_and(|(_, message)| message == expected)
}

fn resolve_checkpoint(
    checkpoints: &HashMap<&str, &OracleCheckpoint>,
    id: &str,
    stack: &mut Vec<String>,
) -> ResolvedCheckpoint {
    assert!(
        !stack.iter().any(|ancestor| ancestor == id),
        "oracle checkpoint alias cycle: {} -> {id}",
        stack.join(" -> ")
    );
    stack.push(id.to_owned());

    let checkpoint = checkpoints
        .get(id)
        .unwrap_or_else(|| panic!("oracle checkpoint alias target does not exist: {id}"));
    let inherited_graph = checkpoint
        .same_graph_as
        .as_deref()
        .map(|target| resolve_checkpoint(checkpoints, target, stack));
    let inherited_rows = checkpoint
        .same_commits_as
        .as_deref()
        .map(|target| resolve_checkpoint(checkpoints, target, stack));

    let resolved = ResolvedCheckpoint {
        rows: checkpoint
            .rows
            .clone()
            .or_else(|| inherited_graph.as_ref().map(|target| target.rows.clone()))
            .or_else(|| inherited_rows.as_ref().map(|target| target.rows.clone()))
            .unwrap_or_else(|| panic!("oracle checkpoint has no rows or row alias: {id}")),
        wip: checkpoint.wip.clone().or_else(|| {
            inherited_graph
                .as_ref()
                .and_then(|target| target.wip.clone())
        }),
        stashes: checkpoint
            .stashes
            .clone()
            .or_else(|| {
                inherited_graph
                    .as_ref()
                    .map(|target| target.stashes.clone())
            })
            .unwrap_or_default(),
    };
    stack.pop();
    resolved
}

async fn compare_checkpoint(
    scenario: &Scenario,
    checkpoint: &OracleCheckpoint,
    resolved: &ResolvedCheckpoint,
    materialized: &MaterializedScenario,
    backend: &GitCliBackend,
) {
    let snapshot = backend
        .snapshot(&materialized.repository)
        .await
        .unwrap_or_else(|error| {
            panic!(
                "{} checkpoint {}: snapshot failed: {error}",
                scenario.id, checkpoint.id
            )
        });
    let history = backend
        .history(
            &materialized.repository,
            &HistoryQuery {
                scope: HistoryScope::AllRefs,
                cursor: None,
                limit: 200,
            },
        )
        .await
        .unwrap_or_else(|error| {
            panic!(
                "{} checkpoint {}: history failed: {error}",
                scenario.id, checkpoint.id
            )
        });
    assert!(
        !history.has_more,
        "{} checkpoint {}: fixture unexpectedly exceeds the 200-row comparison limit",
        scenario.id, checkpoint.id
    );

    let (head_name, head_oid) = match &snapshot.head {
        HeadState::Branch { name, oid } => (name.as_str(), oid.as_str()),
        state => panic!(
            "{} checkpoint {}: expected branch HEAD, got {state:?}",
            scenario.id, checkpoint.id
        ),
    };
    assert_eq!(
        head_name, checkpoint.head,
        "{} checkpoint {}: checked-out branch mismatch",
        scenario.id, checkpoint.id
    );
    let expected_head_oid = scenario
        .refs
        .iter()
        .find(|reference| reference.name == format!("refs/heads/{}", checkpoint.head))
        .and_then(|reference| materialized.oids.get(&reference.target))
        .unwrap_or_else(|| {
            panic!(
                "{} checkpoint {}: cannot resolve oracle HEAD {}",
                scenario.id, checkpoint.id, checkpoint.head
            )
        });
    assert_eq!(
        head_oid, expected_head_oid,
        "{} checkpoint {}: HEAD OID mismatch",
        scenario.id, checkpoint.id
    );

    let ordinary = history
        .commits
        .iter()
        .filter(|commit| commit.stash.is_none())
        .collect::<Vec<_>>();
    compare_ordinary_rows(
        scenario,
        checkpoint,
        &resolved.rows,
        &ordinary,
        &materialized.oids,
    );

    let actual_stashes = history
        .commits
        .iter()
        .enumerate()
        .filter(|(_, commit)| commit.stash.is_some())
        .collect::<Vec<_>>();
    compare_stashes(
        scenario,
        checkpoint,
        &resolved.rows,
        &resolved.stashes,
        &actual_stashes,
        materialized,
        resolved.wip.is_some(),
    );

    let listed_stashes = backend
        .stash_list(&materialized.repository)
        .await
        .unwrap_or_else(|error| {
            panic!(
                "{} checkpoint {}: stash overview failed: {error}",
                scenario.id, checkpoint.id
            )
        });
    assert_eq!(
        listed_stashes.len(),
        resolved.stashes.len(),
        "{} checkpoint {}: collapsed stash overview count mismatch",
        scenario.id,
        checkpoint.id
    );
    assert_eq!(
        snapshot.status.stash_count as usize,
        resolved.stashes.len(),
        "{} checkpoint {}: snapshot stash count mismatch",
        scenario.id,
        checkpoint.id
    );
    for expected in &resolved.stashes {
        let actual = listed_stashes
            .iter()
            .find(|stash| format!("stash@{{{}}}", stash.index) == expected.selector)
            .unwrap_or_else(|| {
                panic!(
                    "{} checkpoint {}: overview has no actionable {}",
                    scenario.id, checkpoint.id, expected.selector
                )
            });
        assert_eq!(
            actual.message, expected.message,
            "{} checkpoint {}: overview label mismatch for {}",
            scenario.id, checkpoint.id, expected.selector
        );
    }

    if let Some(expected_files) = &checkpoint.worktree_files {
        for expected in expected_files {
            let actual = fs::read_to_string(materialized.repository.join(&expected.path))
                .unwrap_or_else(|error| {
                    panic!(
                        "{} checkpoint {}: read worktree file {} failed: {error}",
                        scenario.id, checkpoint.id, expected.path
                    )
                });
            assert_eq!(
                actual, expected.contents,
                "{} checkpoint {}: worktree contents mismatch for {}",
                scenario.id, checkpoint.id, expected.path
            );
        }
    }

    if let Some(expected_raw_stashes) = &checkpoint.raw_stashes {
        let raw = git(
            &materialized.repository,
            &["stash", "list", "--format=%gd%x09%gs"],
            None,
            &[],
        );
        let actual_raw_stashes = raw
            .lines()
            .filter(|line| !line.is_empty())
            .map(|line| {
                line.split_once('\t')
                    .unwrap_or_else(|| panic!("malformed raw stash record: {line:?}"))
            })
            .collect::<Vec<_>>();
        assert_eq!(
            actual_raw_stashes.len(),
            expected_raw_stashes.len(),
            "{} checkpoint {}: raw stash count mismatch",
            scenario.id,
            checkpoint.id
        );
        for (actual, expected) in actual_raw_stashes.iter().zip(expected_raw_stashes) {
            assert_eq!(
                actual.0, expected.selector,
                "{} checkpoint {}: raw stash selector mismatch",
                scenario.id, checkpoint.id
            );
            assert!(
                stash_message_matches(actual.1, &expected.message),
                "{} checkpoint {}: raw stash message mismatch for {}; actual: {:?}",
                scenario.id,
                checkpoint.id,
                expected.selector,
                actual.1
            );
        }
    }

    assert_eq!(
        !snapshot.status.clean,
        resolved.wip.is_some(),
        "{} checkpoint {}: WIP presence mismatch",
        scenario.id,
        checkpoint.id
    );
    if let Some(wip) = &resolved.wip {
        let expected_wip_head = materialized.oids.get(&wip.head).unwrap_or_else(|| {
            panic!(
                "{} checkpoint {}: unknown WIP head {}",
                scenario.id, checkpoint.id, wip.head
            )
        });
        assert_eq!(
            head_oid, expected_wip_head,
            "{} checkpoint {}: WIP is anchored to the wrong HEAD",
            scenario.id, checkpoint.id
        );
        let head_commit = ordinary
            .iter()
            .find(|commit| commit.oid == head_oid)
            .unwrap_or_else(|| {
                panic!(
                    "{} checkpoint {}: HEAD commit is absent from history",
                    scenario.id, checkpoint.id
                )
            });
        assert_eq!(
            head_commit.graph.lane, wip.lane,
            "{} checkpoint {}: WIP/HEAD lane mismatch",
            scenario.id, checkpoint.id
        );
    }
}

fn compare_ordinary_rows(
    scenario: &Scenario,
    checkpoint: &OracleCheckpoint,
    expected_rows: &[OracleRow],
    actual_rows: &[&CommitSummary],
    oids: &BTreeMap<String, String>,
) {
    let mut expected_rows = expected_rows.iter().collect::<Vec<_>>();
    expected_rows.sort_by_key(|row| row.row);
    assert_eq!(
        actual_rows.len(),
        expected_rows.len(),
        "{} checkpoint {}: non-stash row count mismatch; actual rows: {:?}",
        scenario.id,
        checkpoint.id,
        actual_rows
            .iter()
            .map(|commit| (commit.short_oid.as_str(), commit.subject.as_str()))
            .collect::<Vec<_>>()
    );

    for (row_index, (expected, actual)) in expected_rows.into_iter().zip(actual_rows).enumerate() {
        assert_eq!(
            expected.row, row_index,
            "{} checkpoint {}: oracle row index is not contiguous for {}",
            scenario.id, checkpoint.id, expected.commit
        );
        let expected_oid = oids.get(&expected.commit).unwrap_or_else(|| {
            panic!(
                "{} checkpoint {}: unknown oracle commit {}",
                scenario.id, checkpoint.id, expected.commit
            )
        });
        assert_eq!(
            &actual.oid, expected_oid,
            "{} checkpoint {} row {}: commit order mismatch; expected {}",
            scenario.id, checkpoint.id, row_index, expected.commit
        );
        assert_eq!(
            actual.graph.lane, expected.lane,
            "{} checkpoint {} row {} ({}): lane mismatch",
            scenario.id, checkpoint.id, row_index, expected.commit
        );
        assert_eq!(
            actual.graph.edges.len(),
            expected.edges.len(),
            "{} checkpoint {} row {} ({}): edge count mismatch",
            scenario.id,
            checkpoint.id,
            row_index,
            expected.commit
        );
        for (edge_index, expected_edge) in expected.edges.iter().enumerate() {
            assert_eq!(
                expected_edge.parent_index, edge_index,
                "{} checkpoint {} row {} ({}): oracle parent indexes are not contiguous",
                scenario.id, checkpoint.id, row_index, expected.commit
            );
            let actual_edge = &actual.graph.edges[edge_index];
            let expected_parent = oids.get(&expected_edge.parent).unwrap_or_else(|| {
                panic!(
                    "{} checkpoint {}: unknown oracle parent {}",
                    scenario.id, checkpoint.id, expected_edge.parent
                )
            });
            assert_eq!(
                &actual_edge.parent_oid, expected_parent,
                "{} checkpoint {} row {} ({}) parent {}: parent OID mismatch",
                scenario.id, checkpoint.id, row_index, expected.commit, edge_index
            );
            assert_eq!(
                actual_edge.from_lane, expected_edge.from_lane,
                "{} checkpoint {} row {} ({}) parent {}: from_lane mismatch",
                scenario.id, checkpoint.id, row_index, expected.commit, edge_index
            );
            assert_eq!(
                actual_edge.to_lane, expected_edge.to_lane,
                "{} checkpoint {} row {} ({}) parent {}: to_lane mismatch",
                scenario.id, checkpoint.id, row_index, expected.commit, edge_index
            );
            assert_eq!(
                actual_edge.merge,
                expected_edge.parent_index > 0,
                "{} checkpoint {} row {} ({}) parent {}: merge flag mismatch",
                scenario.id,
                checkpoint.id,
                row_index,
                expected.commit,
                edge_index
            );
        }
    }
}

fn compare_stashes(
    scenario: &Scenario,
    checkpoint: &OracleCheckpoint,
    expected_rows: &[OracleRow],
    expected_stashes: &[OracleStash],
    actual_stashes: &[(usize, &CommitSummary)],
    materialized: &MaterializedScenario,
    has_wip: bool,
) {
    let mut expected_stashes = expected_stashes.iter().collect::<Vec<_>>();
    expected_stashes.sort_by_key(|stash| stash.display_row);
    assert_eq!(
        actual_stashes.len(),
        expected_stashes.len(),
        "{} checkpoint {}: stash row count mismatch; actual rows: {:?}",
        scenario.id,
        checkpoint.id,
        actual_stashes
            .iter()
            .map(|(_, commit)| (commit.short_oid.as_str(), commit.subject.as_str()))
            .collect::<Vec<_>>()
    );

    for (stash_index, (expected, (history_index, actual))) in expected_stashes
        .into_iter()
        .zip(actual_stashes.iter().copied())
        .enumerate()
    {
        assert_eq!(
            expected.display_row,
            history_index + usize::from(has_wip),
            "{} checkpoint {} stash {} ({}): absolute display row mismatch",
            scenario.id,
            checkpoint.id,
            stash_index,
            expected.message
        );
        assert_eq!(
            expected.visible_commit_role,
            OracleStashVisibleCommitRole::IndexParent,
            "{} checkpoint {} stash row {} ({}): unsupported visible commit role",
            scenario.id,
            checkpoint.id,
            stash_index,
            expected.message
        );
        assert_eq!(
            actual.subject, expected.message,
            "{} checkpoint {} stash row {}: message/order mismatch",
            scenario.id, checkpoint.id, stash_index
        );
        let stash = actual
            .stash
            .as_ref()
            .expect("filtered stash row has metadata");
        assert_eq!(
            stash.selector, expected.selector,
            "{} checkpoint {} stash row {} ({}): selector mismatch",
            scenario.id, checkpoint.id, stash_index, expected.message
        );
        let visible_revision = format!("{}^2", expected.selector);
        let expected_visible_oid = git(
            &materialized.repository,
            &[
                "rev-parse",
                "--verify",
                "--end-of-options",
                visible_revision.as_str(),
            ],
            None,
            &[],
        );
        assert_eq!(
            actual.oid, expected_visible_oid,
            "{} checkpoint {} stash row {} ({}): visible commit is not the stash index parent",
            scenario.id, checkpoint.id, stash_index, expected.message
        );
        assert_eq!(
            actual.graph.lane, expected.lane,
            "{} checkpoint {} stash row {} ({}): lane mismatch",
            scenario.id, checkpoint.id, stash_index, expected.message
        );
        let expected_parent = materialized.oids.get(&expected.parent).unwrap_or_else(|| {
            panic!(
                "{} checkpoint {}: unknown stash parent {}",
                scenario.id, checkpoint.id, expected.parent
            )
        });
        assert_eq!(
            actual.parent_oids.as_slice(),
            std::slice::from_ref(expected_parent),
            "{} checkpoint {} stash row {} ({}): parent mismatch",
            scenario.id,
            checkpoint.id,
            stash_index,
            expected.message
        );
        assert_eq!(
            actual.graph.edges.len(),
            1,
            "{} checkpoint {} stash row {} ({}): edge count mismatch",
            scenario.id,
            checkpoint.id,
            stash_index,
            expected.message
        );
        let actual_edge = &actual.graph.edges[0];
        assert_eq!(
            &actual_edge.parent_oid, expected_parent,
            "{} checkpoint {} stash row {} ({}): edge parent mismatch",
            scenario.id, checkpoint.id, stash_index, expected.message
        );
        assert_eq!(
            actual_edge.from_lane, expected.lane,
            "{} checkpoint {} stash row {} ({}): edge from_lane mismatch",
            scenario.id, checkpoint.id, stash_index, expected.message
        );
        let expected_parent_lane = expected_rows
            .iter()
            .find(|row| row.commit == expected.parent)
            .unwrap_or_else(|| {
                panic!(
                    "{} checkpoint {}: stash parent {} has no oracle row",
                    scenario.id, checkpoint.id, expected.parent
                )
            })
            .lane;
        assert_eq!(
            actual_edge.to_lane, expected_parent_lane,
            "{} checkpoint {} stash row {} ({}): edge to_lane mismatch",
            scenario.id, checkpoint.id, stash_index, expected.message
        );
        assert!(
            !actual_edge.merge,
            "{} checkpoint {} stash row {} ({}): single-parent stash edge is marked as a merge",
            scenario.id, checkpoint.id, stash_index, expected.message
        );
    }
}

fn materialize_scenario(scenario: &Scenario) -> PathBuf {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock is after Unix epoch")
        .as_millis();
    let workspace_root = manifest_dir
        .parent()
        .and_then(Path::parent)
        .expect("gitcat-git-cli is inside the workspace crates directory");
    let output_root = workspace_root
        .join("target")
        .join("graph-conformance")
        .join("manual")
        .join(format!("{}-{nonce}", scenario.id));
    fs::create_dir_all(&output_root).expect("create graph conformance output directory");

    let materialized = materialize_scenario_in(scenario, &output_root);
    fs::write(
        output_root.join("oid-map.json"),
        serde_json::to_string_pretty(&materialized.oids).expect("serialize fixture oid map"),
    )
    .expect("write fixture oid map");
    materialized.repository
}

fn materialize_scenario_in(scenario: &Scenario, output_root: &Path) -> MaterializedScenario {
    let repository_name = "repository with spaces";
    git(
        output_root,
        &[
            "init",
            "--object-format=sha1",
            "--initial-branch=main",
            repository_name,
        ],
        None,
        &[],
    );
    let repository = output_root.join(repository_name);
    git(
        &repository,
        &["config", "core.autocrlf", "false"],
        None,
        &[],
    );
    git(
        &repository,
        &["config", "commit.gpgSign", "false"],
        None,
        &[],
    );
    git(
        &repository,
        &["config", "user.name", "GitCat Graph Fixture"],
        None,
        &[],
    );
    git(
        &repository,
        &["config", "user.email", "graph-fixture@example.invalid"],
        None,
        &[],
    );

    let empty_tree = git(&repository, &["mktree"], Some(""), &[]);
    let mut oids: BTreeMap<String, String> = BTreeMap::new();
    let mut trees: BTreeMap<String, String> = BTreeMap::new();
    for commit in &scenario.commits {
        let tree = if commit.files.is_empty() {
            commit
                .parents
                .first()
                .and_then(|parent| trees.get(parent))
                .cloned()
                .unwrap_or_else(|| empty_tree.clone())
        } else {
            let mut entries = Vec::with_capacity(commit.files.len());
            for file in &commit.files {
                let blob = git(
                    &repository,
                    &["hash-object", "-w", "--stdin"],
                    Some(&file.contents),
                    &[],
                );
                entries.push(format!("100644 blob {blob}\t{}\n", file.path));
            }
            entries.sort();
            git(&repository, &["mktree"], Some(&entries.concat()), &[])
        };

        let mut arguments = vec!["commit-tree", tree.as_str()];
        for parent in &commit.parents {
            arguments.push("-p");
            arguments.push(
                oids.get(parent.as_str())
                    .unwrap_or_else(|| panic!("validated parent must exist: {parent}"))
                    .as_str(),
            );
        }
        let oid = git(
            &repository,
            &arguments,
            Some(&commit.subject),
            &[
                ("GIT_AUTHOR_NAME", "GitCat Graph Fixture"),
                ("GIT_AUTHOR_EMAIL", "graph-fixture@example.invalid"),
                ("GIT_AUTHOR_DATE", commit.date.as_str()),
                ("GIT_COMMITTER_NAME", "GitCat Graph Fixture"),
                ("GIT_COMMITTER_EMAIL", "graph-fixture@example.invalid"),
                ("GIT_COMMITTER_DATE", commit.date.as_str()),
            ],
        );
        trees.insert(commit.id.clone(), tree);
        oids.insert(commit.id.clone(), oid);
    }

    for reference in &scenario.refs {
        let oid = oids
            .get(reference.target.as_str())
            .expect("validated ref target must exist");
        git(
            &repository,
            &["update-ref", reference.name.as_str(), oid],
            None,
            &[],
        );
    }

    let initial_ref = format!("refs/heads/{}", scenario.initial_branch);
    git(
        &repository,
        &["symbolic-ref", "HEAD", initial_ref.as_str()],
        None,
        &[],
    );
    git(&repository, &["reset", "--hard"], None, &[]);
    for stash in &scenario.initial_stashes {
        for file in &stash.changes {
            let path = repository.join(&file.path);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).expect("create initial stash parent");
            }
            fs::write(path, &file.contents).expect("write initial stash change");
        }
        git(
            &repository,
            &["stash", "push", "--message", stash.message.as_str()],
            None,
            &[
                ("GIT_AUTHOR_NAME", "GitCat Graph Fixture"),
                ("GIT_AUTHOR_EMAIL", "graph-fixture@example.invalid"),
                ("GIT_AUTHOR_DATE", stash.date.as_str()),
                ("GIT_COMMITTER_NAME", "GitCat Graph Fixture"),
                ("GIT_COMMITTER_EMAIL", "graph-fixture@example.invalid"),
                ("GIT_COMMITTER_DATE", stash.date.as_str()),
            ],
        );
    }
    for file in &scenario.worktree {
        let path = repository.join(&file.path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create fixture worktree parent");
        }
        fs::write(path, &file.contents).expect("write fixture worktree file");
    }
    MaterializedScenario { repository, oids }
}

fn git(
    working_directory: &Path,
    arguments: &[&str],
    input: Option<&str>,
    environment: &[(&str, &str)],
) -> String {
    let mut command = Command::new("git");
    for key in [
        "GIT_DIR",
        "GIT_WORK_TREE",
        "GIT_INDEX_FILE",
        "GIT_OBJECT_DIRECTORY",
        "GIT_ALTERNATE_OBJECT_DIRECTORIES",
        "GIT_CONFIG",
        "GIT_CONFIG_GLOBAL",
        "GIT_CONFIG_SYSTEM",
        "GIT_CONFIG_COUNT",
    ] {
        command.env_remove(key);
    }
    command.current_dir(working_directory).args(arguments);
    command.envs(environment.iter().copied());
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    if input.is_some() {
        command.stdin(Stdio::piped());
    }

    let mut child = command.spawn().expect("run fixture git command");
    if let Some(input) = input {
        child
            .stdin
            .take()
            .expect("fixture git stdin is piped")
            .write_all(input.as_bytes())
            .expect("write fixture git stdin");
    }
    let output = child
        .wait_with_output()
        .expect("wait for fixture git command");
    assert!(
        output.status.success(),
        "git {} failed: {}",
        arguments.join(" "),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout)
        .expect("fixture git output is UTF-8")
        .trim()
        .to_owned()
}

fn capture_instructions(scenario: &Scenario, repository: &Path) -> String {
    let mut lines = vec![
        format!("Scenario: {}", scenario.id),
        format!("Repository: {}", repository.display()),
        String::new(),
        "Run each action inside GitKraken 12.4.0 and capture every checkpoint:".to_owned(),
    ];
    for step in &scenario.steps {
        let line = match step {
            ScenarioStep::ApplyStash { message } => {
                format!("APPLY STASH IN GitKraken: {message}")
            }
            ScenarioStep::Checkpoint { id } => format!("CAPTURE {id}"),
            ScenarioStep::Checkout { branch } => format!("CHECKOUT {branch}"),
            ScenarioStep::CreateStash { message } => {
                format!("CREATE STASH IN GitKraken WITH MESSAGE: {message}")
            }
            ScenarioStep::DeleteStash { message } => {
                format!("DELETE STASH IN GitKraken: {message}")
            }
            ScenarioStep::PopStash { message } => {
                format!("POP STASH IN GitKraken: {message}")
            }
            ScenarioStep::ReopenRepository => "CLOSE AND REOPEN ONLY THE REPOSITORY".to_owned(),
            ScenarioStep::RestartApplication => "FULLY EXIT AND RESTART GitKraken".to_owned(),
        };
        lines.push(line);
    }
    lines.push(String::new());
    lines.join("\n")
}

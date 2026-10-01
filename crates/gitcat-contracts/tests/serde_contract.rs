use gitcat_contracts::{
    AppSettings, ConflictContentKind, ConflictExpectedState, ConflictFileContent,
    ConflictFileDetails, ConflictIndexVersion, ConflictLineEnding, ConflictLineEndingPolicy,
    ConflictPreflightResult, ConflictPreflightState, ConflictResolution, ConflictStageIdentity,
    ConflictWorktreeIdentity, ConflictWorktreeKind, DiffTarget, ErrorCode, HistoryScope, PullMode,
    RepositoryId, WorktreeStatus,
};

#[test]
fn pull_mode_is_stable_snake_case() {
    assert_eq!(
        serde_json::to_string(&PullMode::Rebase).unwrap(),
        r#""rebase""#
    );
    assert_eq!(
        serde_json::to_string(&PullMode::FastForwardOnly).unwrap(),
        r#""fast_forward_only""#
    );
}

#[test]
fn tagged_contracts_have_explicit_kinds() {
    let target = DiffTarget::Commit {
        oid: "abc123".into(),
        parent_index: 0,
    };
    let json = serde_json::to_value(target).unwrap();
    assert_eq!(json["kind"], "commit");
    assert_eq!(json["parent_index"], 0);

    let scope = serde_json::to_value(HistoryScope::Ref("main".into())).unwrap();
    assert_eq!(scope["kind"], "ref");
    assert_eq!(scope["value"], "main");
}

#[test]
fn ids_are_json_safe_and_unique() {
    let first = RepositoryId::new();
    let second = RepositoryId::new();
    assert_ne!(first, second);
    let encoded = serde_json::to_string(&first).unwrap();
    let decoded: RepositoryId = serde_json::from_str(&encoded).unwrap();
    assert_eq!(first, decoded);
}

#[test]
fn defaults_include_theme_and_bounded_queries() {
    let settings = AppSettings::default();
    assert_eq!(settings.history_page_size, 200);
    assert_eq!(settings.diff_context_lines, 3);
    assert_eq!(settings.ui_zoom_percent, 110);
    let theme = settings
        .themes
        .iter()
        .find(|theme| theme.id == settings.active_theme_id)
        .unwrap();
    assert!(theme.colors.graph_palette.len() >= 4);
    assert!(theme.colors.accent.starts_with('#'));
}

#[test]
fn error_codes_remain_transport_values() {
    assert_eq!(
        serde_json::to_string(&ErrorCode::StaleSnapshot).unwrap(),
        r#""stale_snapshot""#
    );
}

#[test]
fn default_worktree_status_is_clean() {
    let status = WorktreeStatus::default();
    assert!(status.clean);
    assert!(status.entries.is_empty());
}

#[test]
fn conflict_preflight_has_stable_transport_shape() {
    let result = ConflictPreflightResult {
        target: "origin/main".into(),
        target_oid: "1111111111111111111111111111111111111111".into(),
        state: ConflictPreflightState::Clean,
        conflicting_paths: Vec::new(),
        unavailable_reason: None,
    };
    let json = serde_json::to_value(result).unwrap();
    assert_eq!(json["target"], "origin/main");
    assert_eq!(json["state"], "clean");
    assert_eq!(json["conflicting_paths"], serde_json::json!([]));
    assert!(json.get("unavailable_reason").is_none());
}

#[test]
fn conflict_editor_contract_has_stable_stage_names_and_shape() {
    assert_eq!(
        serde_json::to_string(&ConflictResolution::Ours).unwrap(),
        r#""ours""#
    );
    assert_eq!(
        serde_json::to_string(&ConflictResolution::Theirs).unwrap(),
        r#""theirs""#
    );
    assert_eq!(
        serde_json::to_string(&ConflictResolution::Delete).unwrap(),
        r#""delete""#
    );
    assert_eq!(
        serde_json::to_string(&ConflictLineEndingPolicy::Preserve).unwrap(),
        r#""preserve""#
    );
    assert_eq!(
        serde_json::to_string(&ConflictLineEndingPolicy::CrLf).unwrap(),
        r#""cr_lf""#
    );

    let ours_identity = ConflictStageIdentity {
        oid: "2222222222222222222222222222222222222222".into(),
        mode: "100644".into(),
    };
    let details = ConflictFileDetails {
        path: "src/main.rs".into(),
        expected_state: ConflictExpectedState {
            base: None,
            ours: Some(ours_identity.clone()),
            theirs: None,
            result: ConflictWorktreeIdentity {
                kind: ConflictWorktreeKind::Missing,
                size: None,
                sha256: None,
                line_ending: None,
                mode: None,
            },
        },
        base: None,
        ours: Some(ConflictIndexVersion {
            oid: ours_identity.oid,
            mode: ours_identity.mode,
            content: ConflictFileContent {
                kind: ConflictContentKind::Text,
                size: Some(4),
                text: Some("ours".into()),
                line_ending: Some(ConflictLineEnding::None),
            },
        }),
        theirs: None,
        result: ConflictFileContent {
            kind: ConflictContentKind::Missing,
            size: None,
            text: None,
            line_ending: None,
        },
    };
    let json = serde_json::to_value(details).unwrap();
    assert_eq!(json["path"], "src/main.rs");
    assert_eq!(json["expected_state"]["ours"]["mode"], "100644");
    assert_eq!(json["expected_state"]["result"]["kind"], "missing");
    assert_eq!(json["ours"]["content"]["kind"], "text");
    assert_eq!(json["result"]["kind"], "missing");
    assert!(json.get("base").is_none());
    assert!(json.get("theirs").is_none());
}

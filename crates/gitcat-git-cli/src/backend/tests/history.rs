//! History pages, cursors, search and how stashes appear in them.

use super::*;

#[tokio::test]
async fn repository_read_workflow_uses_machine_formats() {
    let (directory, backend, oid) = committed_repository().await;
    let snapshot = backend
        .snapshot(directory.path())
        .await
        .expect("snapshot repository");
    assert!(snapshot.status.clean);
    assert_eq!(snapshot.local_branches.len(), 1);
    assert_eq!(snapshot.local_branches[0].name, "main");

    let history = backend
        .history(directory.path(), &HistoryQuery::default())
        .await
        .expect("history");
    assert_eq!(history.commits.len(), 1);
    assert_eq!(history.commits[0].oid, oid);
    assert_eq!(history.commits[0].subject, "initial subject");

    let search = backend
        .search_commits(
            directory.path(),
            &CommitSearchQuery {
                query: "BODY TEXT".into(),
                scope: HistoryScope::AllRefs,
                limit: 20,
            },
        )
        .await
        .expect("search body");
    assert_eq!(search.total, 1);
    assert!(search.hits[0].matched_body);

    let unicode_search = backend
        .search_commits(
            directory.path(),
            &CommitSearchQuery {
                query: "ÁRVÍZTŰRŐ".into(),
                scope: HistoryScope::AllRefs,
                limit: 20,
            },
        )
        .await
        .expect("search Unicode body");
    assert_eq!(unicode_search.total, 1);
    assert!(unicode_search.hits[0].matched_body);

    let details = backend
        .commit_details(directory.path(), &oid, 0)
        .await
        .expect("commit details");
    assert_eq!(details.files.len(), 1);
    assert_eq!(details.files[0].new_path, "hello.txt");
    assert_eq!(details.files[0].additions, Some(1));
}

#[tokio::test]
async fn all_refs_history_ignores_foreign_refs_to_pruned_objects() {
    let (directory, backend, oid) = committed_repository().await;
    // Another tool's checkpoint ref whose object was pruned, as Codex leaves behind.
    let foreign = directory.path().join(".git/refs/codex/checkpoints");
    fs::create_dir_all(&foreign).expect("create foreign ref namespace");
    fs::write(
        foreign.join("gone"),
        "d733a34e5816afc70ebbcccd0ad5852848dd2fe6\n",
    )
    .expect("write broken ref");
    // A detached HEAD on a commit no branch reaches must still be walked.
    git(directory.path(), &["checkout", "--detach"]);
    fs::write(directory.path().join("hello.txt"), "detached\n").expect("write change");
    git(directory.path(), &["commit", "-am", "detached subject"]);
    let detached_oid = git_stdout(directory.path(), &["rev-parse", "HEAD"]);

    let page = backend
        .history(
            directory.path(),
            &HistoryQuery {
                scope: HistoryScope::AllRefs,
                cursor: None,
                limit: 50,
            },
        )
        .await
        .expect("history despite a broken foreign ref");
    let oids: Vec<&str> = page.commits.iter().map(|c| c.oid.as_str()).collect();
    assert_eq!(oids, [detached_oid.as_str(), oid.as_str()]);

    let search = backend
        .search_commits(
            directory.path(),
            &CommitSearchQuery {
                query: "subject".into(),
                scope: HistoryScope::AllRefs,
                limit: 20,
            },
        )
        .await
        .expect("search despite a broken foreign ref");
    assert_eq!(search.total, 2);
}

#[tokio::test]
async fn history_shows_one_row_per_stash() {
    let (directory, backend, _) = committed_repository().await;
    fs::write(directory.path().join("hello.txt"), "stashed\n").expect("write tracked change");
    fs::write(directory.path().join("extra.txt"), "untracked\n").expect("write untracked file");
    git(directory.path(), &["stash", "push", "--include-untracked"]);
    let outer_stash_oid = git_stdout(directory.path(), &["rev-parse", "stash@{0}"]);
    let base_oid = git_stdout(directory.path(), &["rev-parse", "stash@{0}^1"]);
    let visible_index_oid = git_stdout(directory.path(), &["rev-parse", "stash@{0}^2"]);
    let untracked_oid = git_stdout(directory.path(), &["rev-parse", "stash@{0}^3"]);

    let page = backend
        .history(
            directory.path(),
            &HistoryQuery {
                scope: HistoryScope::AllRefs,
                cursor: None,
                limit: 50,
            },
        )
        .await
        .expect("history page");

    let stash_rows: Vec<&CommitSummary> = page
        .commits
        .iter()
        .filter(|commit| commit.stash.is_some())
        .collect();
    assert_eq!(stash_rows.len(), 1);
    assert_eq!(stash_rows[0].subject, "WIP on main");
    assert_eq!(stash_rows[0].oid, visible_index_oid);
    assert_eq!(stash_rows[0].parent_oids, vec![base_oid]);
    assert_eq!(stash_rows[0].parent_oids.len(), 1);
    assert_eq!(stash_rows[0].graph.edges.len(), 1);
    assert_eq!(stash_rows[0].graph.lane, 0);
    assert_eq!(stash_rows[0].stash.as_ref().unwrap().index, 0);
    let first_stash_oid = stash_rows[0].oid.clone();
    assert!(
        !page
            .commits
            .iter()
            .any(|commit| commit.oid == outer_stash_oid)
    );
    assert!(
        !page
            .commits
            .iter()
            .any(|commit| commit.oid == untracked_oid)
    );
    assert!(
        !page
            .commits
            .iter()
            .any(|commit| commit.subject.starts_with("index on")
                || commit.subject.starts_with("untracked files on"))
    );

    fs::write(directory.path().join("hello.txt"), "named\n").expect("write second change");
    backend
        .stash_push(directory.path(), Some("layout tweaks"), true)
        .await
        .expect("stash with message");
    let page = backend
        .history(
            directory.path(),
            &HistoryQuery {
                scope: HistoryScope::AllRefs,
                cursor: None,
                limit: 50,
            },
        )
        .await
        .expect("history page after named stash");
    let mut labels: Vec<&str> = page
        .commits
        .iter()
        .filter(|commit| commit.stash.is_some())
        .map(|commit| commit.subject.as_str())
        .collect();
    labels.sort_unstable();
    assert_eq!(labels, vec!["WIP on main", "layout tweaks"]);

    let details = backend
        .commit_details(directory.path(), &first_stash_oid, 0)
        .await
        .expect("stash commit details");
    assert_eq!(details.subject, "WIP on main");
}

#[tokio::test]
async fn shared_stash_index_parent_uses_oldest_label_and_newest_action() {
    let (directory, backend, base_oid) = committed_repository().await;
    let collision_date = "2030-01-01T00:00:00 +0000";

    fs::write(directory.path().join("hello.txt"), "first\ncollision A\n")
        .expect("write collision A");
    git_with_date(
        directory.path(),
        &["stash", "push", "--message", "oracle: collision A"],
        collision_date,
    );
    let older_outer_oid = git_stdout(directory.path(), &["rev-parse", "stash@{0}"]);
    let visible_index_oid = git_stdout(directory.path(), &["rev-parse", "stash@{0}^2"]);

    fs::write(directory.path().join("hello.txt"), "first\ncollision B\n")
        .expect("write collision B");
    git_with_date(
        directory.path(),
        &["stash", "push", "--message", "oracle: collision B"],
        collision_date,
    );
    let newest_outer_oid = git_stdout(directory.path(), &["rev-parse", "stash@{0}"]);
    assert_ne!(newest_outer_oid, older_outer_oid);
    assert_eq!(
        git_stdout(directory.path(), &["rev-parse", "stash@{0}^2"]),
        visible_index_oid
    );
    assert_eq!(
        git_stdout(directory.path(), &["rev-parse", "stash@{1}^2"]),
        visible_index_oid
    );

    let listed = backend
        .stash_list(directory.path())
        .await
        .expect("list collapsed collision");
    assert_eq!(
        listed,
        vec![StashEntry {
            index: 0,
            oid: newest_outer_oid.clone(),
            message: "oracle: collision A".into(),
        }]
    );
    let snapshot = backend
        .snapshot(directory.path())
        .await
        .expect("snapshot collapsed collision");
    assert_eq!(snapshot.status.stash_count, 1);

    let query = HistoryQuery {
        scope: HistoryScope::AllRefs,
        cursor: None,
        limit: 50,
    };
    let page = backend
        .history(directory.path(), &query)
        .await
        .expect("history with collapsed collision");
    let stash_rows = page
        .commits
        .iter()
        .filter(|commit| commit.stash.is_some())
        .collect::<Vec<_>>();
    assert_eq!(stash_rows.len(), 1);
    let stash_row = stash_rows[0];
    assert_eq!(stash_row.oid, visible_index_oid);
    assert_eq!(stash_row.subject, "oracle: collision A");
    assert_eq!(stash_row.parent_oids, vec![base_oid]);
    assert_eq!(stash_row.graph.lane, 0);
    assert_eq!(
        stash_row.stash.as_ref(),
        Some(&StashRef {
            index: 0,
            selector: "stash@{0}".into(),
            oid: newest_outer_oid.clone(),
        })
    );

    let newest_tree_oid = git_stdout(
        directory.path(),
        &["rev-parse", &format!("{newest_outer_oid}^{{tree}}")],
    );
    for lookup_oid in [&visible_index_oid, &newest_outer_oid, &older_outer_oid] {
        let details = backend
            .commit_details(directory.path(), lookup_oid, 0)
            .await
            .expect("collapsed stash details resolve through every identity");
        assert_eq!(details.subject, "oracle: collision A");
        assert_eq!(details.tree_oid, newest_tree_oid);

        let diff = backend
            .diff(
                directory.path(),
                &DiffRequest {
                    target: DiffTarget::Commit {
                        oid: lookup_oid.clone(),
                        parent_index: 0,
                    },
                    path: "hello.txt".into(),
                    context_lines: 3,
                    ignore_whitespace: false,
                    max_bytes: 1024 * 1024,
                    whole_file: false,
                },
            )
            .await
            .expect("collapsed stash diff resolves through every identity");
        assert!(
            diff.hunks.iter().flat_map(|hunk| &hunk.lines).any(|line| {
                line.kind == DiffLineKind::Addition && line.content == "collision B"
            })
        );
        assert!(
            !diff.hunks.iter().flat_map(|hunk| &hunk.lines).any(|line| {
                line.kind == DiffLineKind::Addition && line.content == "collision A"
            })
        );
    }

    for outer_oid in [&newest_outer_oid, &older_outer_oid] {
        let by_outer = backend
            .history(
                directory.path(),
                &HistoryQuery {
                    scope: HistoryScope::Ref(outer_oid.clone()),
                    cursor: None,
                    limit: 1,
                },
            )
            .await
            .expect("history resolves every colliding outer stash");
        assert_eq!(by_outer.commits.len(), 1);
        assert_eq!(by_outer.commits[0].oid, visible_index_oid);
        assert_eq!(by_outer.commits[0].subject, "oracle: collision A");
    }

    let action_oid = stash_row.stash.as_ref().expect("stash action").oid.clone();
    backend
        .stash_apply(directory.path(), &action_oid, true)
        .await
        .expect("pop displayed collision row");
    assert_eq!(
        fs::read_to_string(directory.path().join("hello.txt"))
            .expect("read popped worktree")
            .replace("\r\n", "\n"),
        "first\ncollision B\n"
    );
    assert_eq!(
        git_stdout(directory.path(), &["stash", "list", "--format=%gs"]),
        "On main: oracle: collision A"
    );
    assert_eq!(
        git_stdout(directory.path(), &["rev-parse", "stash@{0}"]),
        older_outer_oid
    );

    let after = backend
        .history(directory.path(), &query)
        .await
        .expect("history after popping newest collision member");
    let remaining = after
        .commits
        .iter()
        .find(|commit| commit.stash.is_some())
        .expect("older stash remains visible");
    assert_eq!(remaining.subject, "oracle: collision A");
    assert_eq!(remaining.stash.as_ref().expect("remaining stash").index, 0);
    assert_eq!(remaining.graph.lane, 1);
}

#[tokio::test]
async fn stash_ref_history_starts_with_the_visible_row_at_limit_one() {
    let (directory, backend, _) = committed_repository().await;
    fs::write(directory.path().join("hello.txt"), "stashed\n").expect("write stash change");
    git(
        directory.path(),
        &["stash", "push", "--message", "visible stash"],
    );
    let outer_oid = git_stdout(directory.path(), &["rev-parse", "stash@{0}"]);
    let visible_oid = git_stdout(directory.path(), &["rev-parse", "stash@{0}^2"]);

    for reference in ["stash", "stash@{0}", outer_oid.as_str()] {
        let page = backend
            .history(
                directory.path(),
                &HistoryQuery {
                    scope: HistoryScope::Ref(reference.into()),
                    cursor: None,
                    limit: 1,
                },
            )
            .await
            .unwrap_or_else(|error| panic!("history for {reference:?} failed: {error}"));

        assert_eq!(page.commits.len(), 1, "history for {reference:?}");
        assert_eq!(
            page.commits[0].oid, visible_oid,
            "history for {reference:?}"
        );
        assert_eq!(page.commits[0].subject, "visible stash");
        assert_eq!(
            page.commits[0]
                .stash
                .as_ref()
                .map(|stash| stash.selector.as_str()),
            Some("stash@{0}")
        );
        assert!(page.has_more);
        assert!(page.next_cursor.is_some());
    }
}

#[tokio::test]
async fn commit_search_virtualizes_latest_and_older_stash_rows() {
    let (directory, backend, base_oid) = committed_repository().await;
    fs::write(directory.path().join("hello.txt"), "older stash\n")
        .expect("write older stash change");
    git_with_date(
        directory.path(),
        &["stash", "push", "--message", "searchable older stash"],
        "2030-01-01T00:00:00 +0000",
    );
    let older_outer_oid = git_stdout(directory.path(), &["rev-parse", "stash@{0}"]);
    let older_visible_oid = git_stdout(directory.path(), &["rev-parse", "stash@{0}^2"]);

    fs::write(directory.path().join("hello.txt"), "latest stash\n")
        .expect("write latest stash change");
    git_with_date(
        directory.path(),
        &["stash", "push", "--message", "searchable latest stash"],
        "2031-01-01T00:00:00 +0000",
    );
    let latest_outer_oid = git_stdout(directory.path(), &["rev-parse", "stash@{0}"]);
    let latest_visible_oid = git_stdout(directory.path(), &["rev-parse", "stash@{0}^2"]);

    let all = backend
        .search_commits(
            directory.path(),
            &CommitSearchQuery {
                query: "SEARCHABLE".into(),
                scope: HistoryScope::AllRefs,
                limit: 10,
            },
        )
        .await
        .expect("search both stash labels");
    assert_eq!(all.total, 3);
    assert!(!all.truncated);
    assert_eq!(
        all.hits
            .iter()
            .map(|hit| (hit.oid.as_str(), hit.subject.as_str()))
            .collect::<Vec<_>>(),
        vec![
            (latest_visible_oid.as_str(), "searchable latest stash"),
            (older_visible_oid.as_str(), "searchable older stash"),
            (base_oid.as_str(), "initial subject"),
        ]
    );
    assert!(all.hits[..2].iter().all(|hit| hit.matched_subject));
    assert!(all.hits[..2].iter().all(|hit| !hit.matched_body));
    assert!(all.hits[2].matched_body);
    assert!(
        all.hits
            .iter()
            .all(|hit| hit.oid != latest_outer_oid && hit.oid != older_outer_oid)
    );

    let limited = backend
        .search_commits(
            directory.path(),
            &CommitSearchQuery {
                query: "searchable".into(),
                scope: HistoryScope::AllRefs,
                limit: 1,
            },
        )
        .await
        .expect("search stash labels with a one-row limit");
    assert_eq!(limited.total, 3);
    assert!(limited.truncated);
    assert_eq!(limited.hits.len(), 1);
    assert_eq!(limited.hits[0].oid, latest_visible_oid);

    let older = backend
        .search_commits(
            directory.path(),
            &CommitSearchQuery {
                query: "older stash".into(),
                scope: HistoryScope::AllRefs,
                limit: 10,
            },
        )
        .await
        .expect("search an older stash label");
    assert_eq!(older.total, 1);
    assert_eq!(older.hits.len(), 1);
    assert_eq!(older.hits[0].oid, older_visible_oid);
}

#[test]
fn applying_the_stash_view_preserves_git_log_order() {
    let summary = |oid: &str| CommitSummary {
        oid: oid.into(),
        short_oid: oid.into(),
        parent_oids: Vec::new(),
        subject: oid.into(),
        body_preview: String::new(),
        author: Identity {
            name: "Test".into(),
            email: "test@example.invalid".into(),
        },
        authored_at: CommitTime {
            seconds: 0,
            offset_minutes: 0,
        },
        committed_at: CommitTime {
            seconds: 0,
            offset_minutes: 0,
        },
        decorations: Vec::new(),
        stash: None,
        graph: GraphCell::default(),
    };
    let mut commits = vec![
        summary("before"),
        summary("stash-1"),
        summary("stash-3"),
        summary("stash-2"),
        summary("after"),
    ];
    let mut stashes = StashGraph::default();
    for index in [1, 3, 2] {
        stashes.commits.insert(
            format!("stash-{index}"),
            StashCommit {
                stash_oid: format!("outer-{index}"),
                base_oid: Some("base".into()),
                reference: StashRef {
                    index,
                    selector: format!("stash@{{{index}}}"),
                    oid: format!("outer-{index}"),
                },
                label: format!("stash {index}"),
            },
        );
    }

    apply_stash_view(&mut commits, &stashes);

    assert_eq!(
        commits
            .iter()
            .map(|commit| commit.oid.as_str())
            .collect::<Vec<_>>(),
        vec!["before", "stash-1", "stash-3", "stash-2", "after"]
    );
}

#[tokio::test]
async fn history_cursor_tracks_refs_and_wip_visibility() {
    let (directory, backend, first_oid) = committed_repository().await;
    fs::write(directory.path().join("hello.txt"), "second\n").expect("write second version");
    backend
        .stage_paths(directory.path(), &["hello.txt".into()])
        .await
        .expect("stage second version");
    backend
        .create_commit(
            directory.path(),
            &CommitOptions {
                message: "second commit".into(),
                amend: false,
                signoff: false,
            },
        )
        .await
        .expect("second commit");
    let first_page = backend
        .history(
            directory.path(),
            &HistoryQuery {
                scope: HistoryScope::AllRefs,
                cursor: None,
                limit: 1,
            },
        )
        .await
        .expect("first history page");
    let cursor = first_page.next_cursor.expect("next cursor");

    fs::write(directory.path().join("uncommitted.txt"), "worktree only\n")
        .expect("write worktree-only file");
    let error = backend
        .history(
            directory.path(),
            &HistoryQuery {
                scope: HistoryScope::AllRefs,
                cursor: Some(cursor),
                limit: 1,
            },
        )
        .await
        .expect_err("clean-to-dirty transition invalidates history cursor");
    assert_eq!(error.code, ErrorCode::StaleSnapshot);

    let dirty_page = backend
        .history(
            directory.path(),
            &HistoryQuery {
                scope: HistoryScope::AllRefs,
                cursor: None,
                limit: 1,
            },
        )
        .await
        .expect("dirty first history page");
    let dirty_cursor = dirty_page.next_cursor.expect("dirty next cursor");

    fs::write(
        directory.path().join("uncommitted.txt"),
        "different worktree contents\n",
    )
    .expect("change dirty worktree without making it clean");
    backend
        .history(
            directory.path(),
            &HistoryQuery {
                scope: HistoryScope::AllRefs,
                cursor: Some(dirty_cursor.clone()),
                limit: 1,
            },
        )
        .await
        .expect("dirty-to-dirty edit keeps history cursor valid");

    fs::remove_file(directory.path().join("uncommitted.txt")).expect("restore clean worktree");
    let error = backend
        .history(
            directory.path(),
            &HistoryQuery {
                scope: HistoryScope::AllRefs,
                cursor: Some(dirty_cursor),
                limit: 1,
            },
        )
        .await
        .expect_err("dirty-to-clean transition invalidates history cursor");
    assert_eq!(error.code, ErrorCode::StaleSnapshot);

    let clean_page = backend
        .history(
            directory.path(),
            &HistoryQuery {
                scope: HistoryScope::AllRefs,
                cursor: None,
                limit: 1,
            },
        )
        .await
        .expect("new clean history page");
    let clean_cursor = clean_page.next_cursor.expect("new clean cursor");

    backend
        .create_branch(directory.path(), "new-ref", &first_oid, false)
        .await
        .expect("create ref");
    let error = backend
        .history(
            directory.path(),
            &HistoryQuery {
                scope: HistoryScope::AllRefs,
                cursor: Some(clean_cursor),
                limit: 1,
            },
        )
        .await
        .expect_err("ref change invalidates cursor");
    assert_eq!(error.code, ErrorCode::StaleSnapshot);
}

#[tokio::test]
async fn history_layout_follows_wip_and_checkout_state() {
    let (directory, backend, base_oid) = committed_repository().await;
    backend
        .create_branch(directory.path(), "feature", &base_oid, false)
        .await
        .expect("create sibling feature branch");

    fs::write(directory.path().join("main.txt"), "main\n").expect("write main change");
    git(directory.path(), &["add", "--", "main.txt"]);
    git_with_date(
        directory.path(),
        &["commit", "--quiet", "--message", "fixture: main tip"],
        "2030-01-01T00:00:00 +0000",
    );
    let main_oid = git_stdout(directory.path(), &["rev-parse", "HEAD"]);

    git(directory.path(), &["checkout", "--quiet", "feature"]);
    fs::write(directory.path().join("feature.txt"), "feature\n").expect("write feature change");
    git(directory.path(), &["add", "--", "feature.txt"]);
    git_with_date(
        directory.path(),
        &["commit", "--quiet", "--message", "fixture: feature tip"],
        "2031-01-01T00:00:00 +0000",
    );
    let feature_oid = git_stdout(directory.path(), &["rev-parse", "HEAD"]);
    git(directory.path(), &["checkout", "--quiet", "main"]);

    let query = HistoryQuery {
        scope: HistoryScope::AllRefs,
        cursor: None,
        limit: 50,
    };
    let clean = backend
        .history(directory.path(), &query)
        .await
        .expect("clean sibling history");
    assert_eq!(
        clean
            .commits
            .iter()
            .map(|commit| commit.oid.as_str())
            .collect::<Vec<_>>(),
        vec![feature_oid.as_str(), main_oid.as_str(), base_oid.as_str()]
    );
    assert_eq!(
        clean
            .commits
            .iter()
            .map(|commit| commit.graph.lane)
            .collect::<Vec<_>>(),
        vec![0, 1, 0]
    );
    assert_eq!(clean.commits[0].graph.edges[0].to_lane, 0);
    assert_eq!(clean.commits[1].graph.edges[0].to_lane, 0);

    fs::write(directory.path().join("wip.txt"), "untracked WIP\n").expect("write WIP");
    let main_wip = backend
        .history(directory.path(), &query)
        .await
        .expect("main WIP sibling history");
    assert_eq!(
        main_wip
            .commits
            .iter()
            .map(|commit| commit.graph.lane)
            .collect::<Vec<_>>(),
        vec![1, 0, 0]
    );
    assert_eq!(main_wip.commits[0].graph.edges[0].to_lane, 0);
    assert_eq!(main_wip.commits[1].graph.edges[0].to_lane, 0);

    backend
        .checkout_branch(directory.path(), "feature")
        .await
        .expect("checkout feature with WIP");
    let feature_wip = backend
        .history(directory.path(), &query)
        .await
        .expect("feature WIP sibling history");
    assert_eq!(
        feature_wip
            .commits
            .iter()
            .map(|commit| commit.graph.lane)
            .collect::<Vec<_>>(),
        vec![0, 1, 0]
    );
    assert_eq!(feature_wip.commits[0].graph.edges[0].to_lane, 0);
    assert_eq!(feature_wip.commits[1].graph.edges[0].to_lane, 0);
}

#[tokio::test]
async fn history_preserves_git_log_order_for_stash_rows() {
    let (directory, backend, _) = committed_repository().await;
    for (label, date, contents) in [
        ("oracle: stash A", "2000-01-01T03:00:00 +0000", "stash A\n"),
        ("oracle: stash B", "2000-01-01T01:00:00 +0000", "stash B\n"),
        ("oracle: stash C", "2000-01-01T02:00:00 +0000", "stash C\n"),
    ] {
        fs::write(directory.path().join("hello.txt"), contents).expect("write stash fixture");
        git_with_date(
            directory.path(),
            &["stash", "push", "--message", label],
            date,
        );
    }

    let page = backend
        .history(
            directory.path(),
            &HistoryQuery {
                scope: HistoryScope::AllRefs,
                cursor: None,
                limit: 50,
            },
        )
        .await
        .expect("history with dated stashes");
    let stashes = page
        .commits
        .iter()
        .filter_map(|commit| {
            commit
                .stash
                .as_ref()
                .map(|stash| (commit.subject.as_str(), stash.selector.as_str()))
        })
        .collect::<Vec<_>>();

    assert_eq!(
        stashes,
        vec![
            ("oracle: stash A", "stash@{2}"),
            ("oracle: stash C", "stash@{0}"),
            ("oracle: stash B", "stash@{1}"),
        ]
    );
}

#[tokio::test]
async fn history_cursor_tracks_stash_identity_and_selectors() {
    let (directory, backend, _) = committed_repository().await;
    fs::write(directory.path().join("hello.txt"), "second\n").expect("write second version");
    backend
        .stage_paths(directory.path(), &["hello.txt".into()])
        .await
        .expect("stage second version");
    backend
        .create_commit(
            directory.path(),
            &CommitOptions {
                message: "second commit".into(),
                amend: false,
                signoff: false,
            },
        )
        .await
        .expect("second commit");

    for (label, contents) in [("stash A", "stash A\n"), ("stash B", "stash B\n")] {
        fs::write(directory.path().join("hello.txt"), contents).expect("write stash change");
        backend
            .stash_push(directory.path(), Some(label), false)
            .await
            .expect("create stash");
    }

    let first_page = backend
        .history(
            directory.path(),
            &HistoryQuery {
                scope: HistoryScope::AllRefs,
                cursor: None,
                limit: 1,
            },
        )
        .await
        .expect("first page with stashes");
    let cursor = first_page.next_cursor.expect("stash history cursor");

    git(directory.path(), &["stash", "drop", "stash@{0}"]);
    let error = backend
        .history(
            directory.path(),
            &HistoryQuery {
                scope: HistoryScope::AllRefs,
                cursor: Some(cursor),
                limit: 1,
            },
        )
        .await
        .expect_err("stash deletion and selector shift invalidate cursor");
    assert_eq!(error.code, ErrorCode::StaleSnapshot);
}

#[tokio::test]
async fn stash_details_and_diff_compare_the_base_to_the_outer_stash() {
    let (directory, backend, base_oid) = committed_repository().await;
    fs::write(directory.path().join("hello.txt"), "unstaged-only change\n")
        .expect("write unstaged-only stash change");
    backend
        .stash_push(directory.path(), Some("unstaged-only stash"), false)
        .await
        .expect("stash unstaged-only change");
    let visible_index_oid = git_stdout(directory.path(), &["rev-parse", "stash@{0}^2"]);
    let outer_stash_oid = git_stdout(directory.path(), &["rev-parse", "stash@{0}"]);
    let visible_short_oid = git_stdout(
        directory.path(),
        &["rev-parse", "--short", &visible_index_oid],
    );
    let outer_tree_oid = git_stdout(directory.path(), &["rev-parse", "stash@{0}^{tree}"]);
    assert_eq!(
        git_stdout(directory.path(), &["rev-parse", "stash@{0}^1"]),
        base_oid
    );
    assert!(
        git_stdout(
            directory.path(),
            &["diff", "--name-only", &base_oid, &visible_index_oid]
        )
        .is_empty(),
        "the visible index parent intentionally has no staged diff"
    );
    assert_eq!(
        git_stdout(
            directory.path(),
            &["diff", "--name-only", &base_oid, &outer_stash_oid]
        ),
        "hello.txt"
    );

    let details = backend
        .commit_details(directory.path(), &visible_index_oid, 0)
        .await
        .expect("stash details");
    assert_eq!(details.oid, visible_index_oid);
    assert_eq!(details.short_oid, visible_short_oid);
    assert_eq!(details.tree_oid, outer_tree_oid);
    assert_eq!(details.parent_oids, vec![base_oid.clone()]);
    assert_eq!(details.subject, "unstaged-only stash");
    assert_eq!(details.files.len(), 1);
    assert_eq!(details.files[0].new_path, "hello.txt");
    assert_eq!(details.files[0].status, ChangeKind::Modified);

    let diff = backend
        .diff(
            directory.path(),
            &DiffRequest {
                target: DiffTarget::Commit {
                    oid: visible_index_oid,
                    parent_index: 0,
                },
                path: "hello.txt".into(),
                context_lines: 3,
                ignore_whitespace: false,
                max_bytes: 1024 * 1024,
                whole_file: false,
            },
        )
        .await
        .expect("stash file diff");
    assert_eq!(diff.status, ChangeKind::Modified);
    assert_eq!(diff.stats.files, 1);
    assert!(!diff.hunks.is_empty());
}

// The offer to make the first commit is drawn where the graph would be, so it
// only appears if an empty repository pages as an empty history rather than as
// an error.
#[tokio::test]
async fn history_of_an_unborn_repository_is_an_empty_page() {
    let directory = tempdir().expect("temp repository");
    let backend = GitCliBackend::default();
    backend
        .init_repository(directory.path(), "main")
        .await
        .expect("initialize repository");

    let page = backend
        .history(
            directory.path(),
            &HistoryQuery {
                scope: HistoryScope::AllRefs,
                limit: 50,
                cursor: None,
            },
        )
        .await
        .expect("history of an unborn repository");

    assert!(page.commits.is_empty());
    assert!(!page.has_more);
}

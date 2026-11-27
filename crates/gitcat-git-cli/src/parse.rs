use std::collections::{HashMap, HashSet};

use gitcat_contracts::{
    ApiError, ApiResult, BranchInfo, ChangeKind, ChangedFile, CommitDetails, CommitSearchHit,
    CommitSummary, CommitTime, ConflictKind, DiffHunk, DiffLine, DiffLineKind, DiffStats, FileDiff,
    GraphCell, HeadState, Identity, LineStats, RefKind, RefLabel, StashEntry, StashRef,
    StatusEntry, WorktreeStatus,
};

use gitcat_contracts::ErrorCode;

mod diff;
mod log;
mod refs;
mod stash;
mod status;

pub(crate) use diff::*;
pub(crate) use log::*;
pub(crate) use refs::*;
pub(crate) use stash::*;
pub(crate) use status::*;

pub(crate) const REF_FORMAT: &str = "%(refname)%00%(refname:short)%00%(objectname)%00%(*objectname)%00%(HEAD)%00%(upstream:short)%00%(upstream:track)%00%(symref)%00%(objecttype)";
// Git pretty formats have no length-prefixed field mode. NUL/RS are robust for normal commits,
// but a deliberately forged message containing these control bytes remains a documented P2 case.
pub(crate) const LOG_FORMAT: &str =
    "%x1e%H%x00%h%x00%P%x00%an%x00%ae%x00%at%x00%ai%x00%ct%x00%ci%x00%s%x00%b%x00";
pub(crate) const DETAIL_FORMAT: &str = "%x1e%H%x00%h%x00%T%x00%P%x00%an%x00%ae%x00%at%x00%ai%x00%cn%x00%ce%x00%ct%x00%ci%x00%s%x00%b%x00";
pub(crate) const STASH_GRAPH_FORMAT: &str = "%gd%x00%H%x00%P%x00%gs%x00";

pub(crate) fn parse_git_version(raw: &[u8]) -> ApiResult<(u32, u32, u32, String)> {
    let raw = text(raw).trim().to_owned();
    let number = raw
        .split_whitespace()
        .find(|part| part.as_bytes().first().is_some_and(u8::is_ascii_digit))
        .ok_or_else(|| parse_error("Git version output did not contain a version number"))?;
    let mut values = number.split('.').map(|part| {
        part.chars()
            .take_while(char::is_ascii_digit)
            .collect::<String>()
            .parse::<u32>()
            .unwrap_or(0)
    });
    Ok((
        values.next().unwrap_or(0),
        values.next().unwrap_or(0),
        values.next().unwrap_or(0),
        raw,
    ))
}

fn split_oids(value: &[u8]) -> Vec<String> {
    text(value).split_whitespace().map(str::to_owned).collect()
}

fn nonempty(value: &[u8]) -> Option<String> {
    (!value.is_empty()).then(|| text(value).into_owned())
}

fn split_once(value: &[u8], needle: u8) -> Option<(&[u8], &[u8])> {
    let index = value.iter().position(|byte| *byte == needle)?;
    Some((&value[..index], &value[index + 1..]))
}

fn preview(value: &str, max_chars: usize) -> String {
    let mut chars = value.chars();
    let preview: String = chars.by_ref().take(max_chars).collect();
    if chars.next().is_some() {
        format!("{preview}…")
    } else {
        preview
    }
}

fn excerpt_around(value: &str, query_lower: &str, max_chars: usize) -> String {
    let lower = value.to_lowercase();
    let byte_match = lower.find(query_lower).unwrap_or(0);
    let match_char = lower[..byte_match].chars().count();
    let start_char = match_char.saturating_sub(max_chars / 3);
    let total_chars = value.chars().count();
    let end_char = (start_char + max_chars).min(total_chars);
    let content: String = value
        .chars()
        .skip(start_char)
        .take(end_char - start_char)
        .collect();
    match (start_char > 0, end_char < total_chars) {
        (true, true) => format!("…{content}…"),
        (true, false) => format!("…{content}"),
        (false, true) => format!("{content}…"),
        (false, false) => content,
    }
}

fn trim_record(mut value: &[u8]) -> &[u8] {
    while matches!(value.last(), Some(b'\n' | b'\r')) {
        value = &value[..value.len() - 1];
    }
    value
}

fn trim_line(mut value: &[u8]) -> &[u8] {
    while matches!(value.last(), Some(b'\n' | b'\r')) {
        value = &value[..value.len() - 1];
    }
    value
}

fn text(value: &[u8]) -> std::borrow::Cow<'_, str> {
    String::from_utf8_lossy(value)
}

fn parse_error(message: &'static str) -> ApiError {
    ApiError::new(ErrorCode::GitCommandFailed, message)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_porcelain_v2_with_rename_and_conflict() {
        let bytes = b"# branch.oid 0123456789abcdef\0# branch.head main\0# branch.upstream origin/main\0# branch.ab +2 -1\0# stash 3\x001 M. N... 100644 100644 100644 aaaaaaa bbbbbbb src/a.rs\x002 R. N... 100644 100644 100644 aaaaaaa bbbbbbb R100 src/new name.rs\0src/old name.rs\0u UU N... 100644 100644 100644 100644 a b c d conflict.rs\0? new.txt\0";
        let parsed = parse_status(bytes).unwrap();
        assert_eq!(parsed.status.ahead, 2);
        assert_eq!(parsed.status.behind, 1);
        assert_eq!(parsed.status.stash_count, 3);
        assert_eq!(parsed.status.entries.len(), 4);
        assert_eq!(
            parsed.status.entries[1].old_path.as_deref(),
            Some("src/old name.rs")
        );
        assert!(parsed.status.entries[2].conflicted);
        assert_eq!(
            parsed.status.entries[2].conflict_kind,
            Some(ConflictKind::BothModified)
        );
        assert_eq!(parsed.status.entries[0].conflict_kind, None);
    }

    #[test]
    fn classifies_every_unmerged_status_code() {
        let cases = [
            ("UU", ConflictKind::BothModified),
            ("AA", ConflictKind::BothAdded),
            ("DD", ConflictKind::BothDeleted),
            ("AU", ConflictKind::AddedByUs),
            ("UA", ConflictKind::AddedByThem),
            ("DU", ConflictKind::DeletedByUs),
            ("UD", ConflictKind::DeletedByThem),
        ];
        for (xy, kind) in cases {
            let record = format!(
                "# branch.oid 0123456789abcdef\0# branch.head main\0u {xy} N... 100644 100644 100644 100644 a b c d file.txt\0"
            );
            let parsed = parse_status(record.as_bytes()).unwrap();
            let entry = &parsed.status.entries[0];
            assert!(entry.conflicted, "{xy} is a conflict");
            assert_eq!(entry.conflict_kind, Some(kind), "{xy}");
            assert!(!entry.submodule);
        }
        let submodule = parse_status(
            b"# branch.oid 0123456789abcdef\0# branch.head main\0u UU S... 160000 160000 160000 160000 a b c d libmod\0",
        )
        .unwrap();
        assert!(submodule.status.entries[0].submodule);
        assert_eq!(
            submodule.status.entries[0].conflict_kind,
            Some(ConflictKind::BothModified)
        );
    }

    #[test]
    fn parses_log_record_and_timezone() {
        let bytes = b"\x1e0123456789abcdef\0abcdef0\0parent\0Ada\0ada@example.test\x001700000000\x002023-11-14 22:13:20 +0230\x001700000001\x002023-11-14 22:13:21 +0230\0subject\0body text\0\n";
        let commits = parse_log(bytes).unwrap();
        assert_eq!(commits.len(), 1);
        assert_eq!(commits[0].subject, "subject");
        assert_eq!(commits[0].authored_at.offset_minutes, 150);
    }

    #[test]
    fn parses_stash_graph_with_visible_index_commits_and_hidden_plumbing() {
        let bytes = b"stash@{0}\0aaa\0base index untracked\0WIP on main: 400e481 feat: header\0\nstash@{1}\0bbb\0base2 index2\0On feature/x: cleanup: drop dead code\0\nstash@{2}\0ccc\0base3\0On malformed: one parent fallback\0\n";
        let graph = parse_stash_graph(bytes).unwrap();
        assert_eq!(graph.commits.len(), 3);

        let first = &graph.commits["index"];
        assert_eq!(first.label, "WIP on main");
        assert_eq!(first.reference.index, 0);
        assert_eq!(first.stash_oid, "aaa");
        assert_eq!(first.base_oid.as_deref(), Some("base"));

        let second = &graph.commits["index2"];
        assert_eq!(second.label, "cleanup: drop dead code");
        assert_eq!(second.reference.selector, "stash@{1}");
        assert_eq!(second.stash_oid, "bbb");
        assert_eq!(second.base_oid.as_deref(), Some("base2"));

        let fallback = &graph.commits["ccc"];
        assert_eq!(fallback.label, "one parent fallback");
        assert_eq!(fallback.stash_oid, "ccc");
        assert_eq!(fallback.base_oid.as_deref(), Some("base3"));

        assert_eq!(graph.hidden.len(), 3);
        assert!(graph.hidden.contains("aaa"));
        assert!(graph.hidden.contains("bbb"));
        assert!(graph.hidden.contains("untracked"));
        assert!(!graph.hidden.contains("base"));
        assert!(!graph.hidden.contains("base2"));
        assert!(!graph.hidden.contains("base3"));
        assert!(!graph.hidden.contains("index"));
        assert!(!graph.hidden.contains("index2"));
        assert!(!graph.hidden.contains("ccc"));
        assert_eq!(graph.outer_to_visible["aaa"], "index");
        assert_eq!(graph.outer_to_visible["bbb"], "index2");
        assert_eq!(graph.outer_to_visible["ccc"], "ccc");
    }

    #[test]
    fn shared_index_parent_keeps_oldest_label_and_newest_action_identity() {
        let bytes = b"stash@{0}\0outer-b\0base shared-index\0On main: oracle: collision B\0\nstash@{1}\0outer-a\0base shared-index\0On main: oracle: collision A\0\n";
        let graph = parse_stash_graph(bytes).unwrap();
        assert_eq!(graph.commits.len(), 1);

        let stash = &graph.commits["shared-index"];
        assert_eq!(stash.label, "oracle: collision A");
        assert_eq!(stash.reference.index, 0);
        assert_eq!(stash.reference.selector, "stash@{0}");
        assert_eq!(stash.stash_oid, "outer-b");
        assert_eq!(stash.base_oid.as_deref(), Some("base"));
        assert_eq!(graph.outer_to_visible["outer-a"], "shared-index");
        assert_eq!(graph.outer_to_visible["outer-b"], "shared-index");
        assert!(graph.hidden.contains("outer-a"));
        assert!(graph.hidden.contains("outer-b"));

        assert_eq!(
            graph.entries(),
            vec![StashEntry {
                index: 0,
                oid: "outer-b".into(),
                message: "oracle: collision A".into(),
            }]
        );
    }

    #[test]
    fn parses_structured_diff() {
        let patch = b"diff --git a/a.txt b/a.txt\nindex 123..456 100644\n--- a/a.txt\n+++ b/a.txt\n@@ -1,2 +1,2 @@\n same\n-old\n+new\n";
        let diff = parse_file_diff(patch, "a.txt", false).unwrap();
        assert_eq!(diff.hunks.len(), 1);
        assert_eq!(diff.stats.additions, 1);
        assert_eq!(diff.stats.deletions, 1);
        assert_eq!(diff.hunks[0].lines[1].old_line, Some(2));
        assert_eq!(diff.hunks[0].lines[2].new_line, Some(2));
    }

    #[test]
    fn rejects_multiple_files_in_single_file_diff_contract() {
        let patch = b"diff --git a/a.txt b/a.txt\n--- a/a.txt\n+++ b/a.txt\ndiff --git a/b.txt b/b.txt\n--- a/b.txt\n+++ b/b.txt\n";
        let error = parse_file_diff(patch, ".", false).expect_err("directory diff rejected");
        assert_eq!(error.code, ErrorCode::InvalidRequest);
    }

    #[test]
    fn parses_name_status_and_numstat() {
        let names = b"M\0a.txt\0R090\0old.rs\0new.rs\0";
        let stats = b"2\t1\ta.txt\x001\t0\t\0old.rs\0new.rs\0";
        let files = parse_changed_files(names, stats).unwrap();
        assert_eq!(files.len(), 2);
        assert_eq!(files[1].status, ChangeKind::Renamed);
        assert_eq!(files[1].old_path.as_deref(), Some("old.rs"));
        assert_eq!(files[1].additions, Some(1));
    }

    #[test]
    fn numstat_renames_keep_only_paired_records() {
        let renames =
            parse_numstat_renames(b"1	0	 old.rs new.rs 3	1	other.rs -	-	 a.png b.png ").unwrap();
        assert_eq!(renames.len(), 2);
        assert_eq!(renames[0].old_path, "old.rs");
        assert_eq!(renames[0].new_path, "new.rs");
        assert_eq!(
            renames[0].stats,
            Some(LineStats {
                additions: 1,
                deletions: 0
            })
        );
        assert_eq!(renames[1].stats, None);
    }

    #[test]
    fn worktree_rename_keeps_a_source_that_still_has_a_staged_change() {
        let mut status = parse_status(
            b"1 MD N... 100644 100644 000000 aaaaaaa bbbbbbb old.rs ? new.rs ? alone.rs ",
        )
        .unwrap()
        .status;
        merge_worktree_renames(
            &mut status,
            &[DetectedRename {
                old_path: "old.rs".into(),
                new_path: "new.rs".into(),
                stats: None,
            }],
        );
        assert_eq!(status.entries.len(), 3);
        assert_eq!(status.entries[0].index, Some(ChangeKind::Modified));
        assert_eq!(status.entries[0].worktree, None);
        assert_eq!(status.entries[1].worktree, Some(ChangeKind::Renamed));
        assert_eq!(status.entries[1].old_path.as_deref(), Some("old.rs"));
        assert_eq!(status.entries[2].worktree, Some(ChangeKind::Untracked));
    }
}

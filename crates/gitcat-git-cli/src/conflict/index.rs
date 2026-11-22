//! The index side of a conflict: unmerged stages, the expected state, merge-tree preflight.

use super::*;

pub(crate) const GITLINK_MODE: &str = "160000";

pub(crate) fn parse_unmerged_index_entries(
    stdout: &[u8],
    expected_path: &str,
) -> ApiResult<Vec<UnmergedIndexEntry>> {
    let mut entries = Vec::new();
    for record in stdout
        .split(|byte| *byte == 0)
        .filter(|record| !record.is_empty())
    {
        let tab = record
            .iter()
            .position(|byte| *byte == b'\t')
            .ok_or_else(|| {
                ApiError::new(
                    ErrorCode::GitCommandFailed,
                    "Git returned malformed unmerged index data",
                )
            })?;
        if &record[tab + 1..] != expected_path.as_bytes() {
            return Err(ApiError::new(
                ErrorCode::GitCommandFailed,
                "Git returned an unexpected unmerged path",
            ));
        }
        let header = std::str::from_utf8(&record[..tab]).map_err(|error| {
            ApiError::new(
                ErrorCode::GitCommandFailed,
                "Git returned non-UTF-8 unmerged index metadata",
            )
            .with_details(error.to_string())
        })?;
        let mut fields = header.split_ascii_whitespace();
        let mode = fields.next().unwrap_or_default();
        let oid = fields.next().unwrap_or_default();
        let stage = fields
            .next()
            .unwrap_or_default()
            .parse::<u8>()
            .map_err(|error| {
                ApiError::new(
                    ErrorCode::GitCommandFailed,
                    "Git returned an invalid conflict stage",
                )
                .with_details(error.to_string())
            })?;
        if fields.next().is_some()
            || !matches!(stage, 1..=3)
            || mode.len() != 6
            || !mode.bytes().all(|byte| byte.is_ascii_digit())
            || !is_full_oid(oid)
            || entries
                .iter()
                .any(|entry: &UnmergedIndexEntry| entry.stage == stage)
        {
            return Err(ApiError::new(
                ErrorCode::GitCommandFailed,
                "Git returned inconsistent unmerged index metadata",
            ));
        }
        entries.push(UnmergedIndexEntry {
            mode: mode.to_owned(),
            oid: oid.to_owned(),
            stage,
        });
    }
    if entries.is_empty() {
        return Err(ApiError::new(
            ErrorCode::StaleSnapshot,
            "Conflict index entries no longer exist",
        )
        .with_details(expected_path.to_owned()));
    }
    entries.sort_unstable_by_key(|entry| entry.stage);
    Ok(entries)
}

pub(crate) fn entry_at_stage(
    entries: &[UnmergedIndexEntry],
    stage: u8,
) -> Option<&UnmergedIndexEntry> {
    entries.iter().find(|entry| entry.stage == stage)
}

pub(crate) fn conflict_expected_state(
    entries: &[UnmergedIndexEntry],
    result: ConflictWorktreeIdentity,
) -> ConflictExpectedState {
    let identity = |stage| {
        entry_at_stage(entries, stage).map(|entry| ConflictStageIdentity {
            oid: entry.oid.clone(),
            mode: entry.mode.clone(),
        })
    };
    ConflictExpectedState {
        base: identity(1),
        ours: identity(2),
        theirs: identity(3),
        result,
    }
}

pub(crate) fn summarize_paths(paths: &[&String]) -> String {
    let listed = paths
        .iter()
        .take(MAX_REPORTED_BULK_PATHS)
        .map(|path| path.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    if paths.len() > MAX_REPORTED_BULK_PATHS {
        format!(
            "{listed} and {} more",
            paths.len() - MAX_REPORTED_BULK_PATHS
        )
    } else {
        listed
    }
}

pub(crate) fn supports_merge_tree_preflight(version: &GitVersion) -> bool {
    version.major > 2 || (version.major == 2 && version.minor >= 38)
}

pub(crate) fn merge_tree_preflight_unavailable(output: &GitCommandOutput) -> bool {
    let diagnostic = format!(
        "{}\n{}",
        output.stderr_lossy_redacted(),
        output.stdout_lossy()
    )
    .to_ascii_lowercase();
    output.status.code() == Some(129)
        || diagnostic.contains("unknown option")
        || diagnostic.contains("unrecognized option")
        || diagnostic.contains("unknown switch")
}

pub(crate) fn parse_merge_tree_paths(stdout: &[u8]) -> ApiResult<Vec<String>> {
    let mut fields = stdout.split(|byte| *byte == 0);
    let tree_oid = fields
        .next()
        .map(String::from_utf8_lossy)
        .map(|value| value.trim().to_owned())
        .unwrap_or_default();
    if !is_full_oid(&tree_oid) {
        return Err(ApiError::new(
            ErrorCode::GitCommandFailed,
            "Git conflict preflight did not return a valid tree object ID",
        ));
    }

    let mut paths: Vec<String> = fields
        .filter(|field| !field.is_empty())
        .map(|field| String::from_utf8_lossy(field).into_owned())
        .collect();
    paths.sort_unstable();
    paths.dedup();
    Ok(paths)
}

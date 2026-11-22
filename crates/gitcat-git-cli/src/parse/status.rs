//! Porcelain v2 status records.

use super::*;

#[derive(Debug)]
pub(crate) struct ParsedStatus {
    pub head: HeadState,
    pub status: WorktreeStatus,
}
pub(crate) fn parse_status(output: &[u8]) -> ApiResult<ParsedStatus> {
    let mut branch_oid: Option<String> = None;
    let mut branch_name: Option<String> = None;
    let mut ahead = 0;
    let mut behind = 0;
    let mut stash_count = 0;
    let mut entries = Vec::new();
    let records: Vec<&[u8]> = output.split(|byte| *byte == 0).collect();
    let mut index = 0;

    while index < records.len() {
        let record = records[index];
        index += 1;
        if record.is_empty() {
            continue;
        }
        if let Some(value) = record.strip_prefix(b"# branch.oid ") {
            if value != b"(initial)" {
                branch_oid = Some(text(value).trim().to_owned());
            }
            continue;
        }
        if let Some(value) = record.strip_prefix(b"# branch.head ") {
            branch_name = Some(text(value).trim().to_owned());
            continue;
        }
        if let Some(value) = record.strip_prefix(b"# branch.upstream ") {
            let _ = value;
            continue;
        }
        if let Some(value) = record.strip_prefix(b"# branch.ab ") {
            let value = text(value);
            for part in value.split_whitespace() {
                if let Some(number) = part.strip_prefix('+') {
                    ahead = number.parse().unwrap_or(0);
                } else if let Some(number) = part.strip_prefix('-') {
                    behind = number.parse().unwrap_or(0);
                }
            }
            continue;
        }
        if let Some(value) = record.strip_prefix(b"# stash ") {
            stash_count = text(value).trim().parse().unwrap_or(0);
            continue;
        }
        if record.starts_with(b"# ") {
            continue;
        }

        match record.first().copied() {
            Some(b'1') => entries.push(parse_ordinary_status(record)?),
            Some(b'2') => {
                let mut entry = parse_renamed_status(record)?;
                let old_path = records.get(index).ok_or_else(|| {
                    parse_error("Renamed status entry did not contain its original path")
                })?;
                index += 1;
                entry.old_path = Some(text(old_path).into_owned());
                entries.push(entry);
            }
            Some(b'u') => entries.push(parse_unmerged_status(record)?),
            Some(b'?') => entries.push(StatusEntry {
                path: text(record.get(2..).unwrap_or_default()).into_owned(),
                old_path: None,
                index: None,
                worktree: Some(ChangeKind::Untracked),
                conflicted: false,
                conflict_kind: None,
                submodule: false,
                index_stats: None,
                worktree_stats: None,
            }),
            Some(b'!') => entries.push(StatusEntry {
                path: text(record.get(2..).unwrap_or_default()).into_owned(),
                old_path: None,
                index: None,
                worktree: Some(ChangeKind::Ignored),
                conflicted: false,
                conflict_kind: None,
                submodule: false,
                index_stats: None,
                worktree_stats: None,
            }),
            _ => return Err(parse_error("Unknown porcelain v2 status record")),
        }
    }

    let head_name = branch_name.unwrap_or_else(|| "(detached)".to_owned());
    let head = match (branch_oid, head_name.as_str()) {
        (Some(oid), "(detached)") => HeadState::Detached { oid },
        (Some(oid), name) => HeadState::Branch {
            name: name.to_owned(),
            oid,
        },
        (None, name) => HeadState::Unborn {
            intended_branch: name.to_owned(),
        },
    };
    Ok(ParsedStatus {
        head,
        status: WorktreeStatus {
            clean: entries.is_empty(),
            ahead,
            behind,
            stash_count,
            entries,
        },
    })
}

pub(super) fn parse_ordinary_status(record: &[u8]) -> ApiResult<StatusEntry> {
    let fields: Vec<&[u8]> = record.splitn(9, |byte| *byte == b' ').collect();
    if fields.len() != 9 {
        return Err(parse_error("Malformed ordinary porcelain v2 status record"));
    }
    status_entry(fields[1], fields[2], fields[8], false)
}

pub(super) fn parse_renamed_status(record: &[u8]) -> ApiResult<StatusEntry> {
    let fields: Vec<&[u8]> = record.splitn(10, |byte| *byte == b' ').collect();
    if fields.len() != 10 {
        return Err(parse_error("Malformed renamed porcelain v2 status record"));
    }
    status_entry(fields[1], fields[2], fields[9], false)
}

pub(super) fn parse_unmerged_status(record: &[u8]) -> ApiResult<StatusEntry> {
    let fields: Vec<&[u8]> = record.splitn(11, |byte| *byte == b' ').collect();
    if fields.len() != 11 {
        return Err(parse_error("Malformed unmerged porcelain v2 status record"));
    }
    status_entry(fields[1], fields[2], fields[10], true)
}

pub(super) fn status_entry(
    xy: &[u8],
    submodule: &[u8],
    path: &[u8],
    forced_conflict: bool,
) -> ApiResult<StatusEntry> {
    if xy.len() != 2 {
        return Err(parse_error("Porcelain v2 XY field was malformed"));
    }
    let index = change_from_status(xy[0]);
    let worktree = change_from_status(xy[1]);
    let conflicted = forced_conflict || xy.contains(&b'U') || matches!(xy, b"AA" | b"DD");
    Ok(StatusEntry {
        path: text(path).into_owned(),
        old_path: None,
        index,
        worktree,
        conflicted,
        conflict_kind: if conflicted { conflict_kind(xy) } else { None },
        submodule: !submodule.starts_with(b"N..."),
        index_stats: None,
        worktree_stats: None,
    })
}

/// Porcelain v2 spells every unmerged shape as an XY pair; `U` marks the side
/// that modified the path, `A` and `D` the side that added or deleted it.
pub(crate) fn conflict_kind(xy: &[u8]) -> Option<ConflictKind> {
    Some(match xy {
        b"UU" => ConflictKind::BothModified,
        b"AA" => ConflictKind::BothAdded,
        b"DD" => ConflictKind::BothDeleted,
        b"AU" => ConflictKind::AddedByUs,
        b"UA" => ConflictKind::AddedByThem,
        b"DU" => ConflictKind::DeletedByUs,
        b"UD" => ConflictKind::DeletedByThem,
        _ => return None,
    })
}

pub(super) fn change_from_status(value: u8) -> Option<ChangeKind> {
    match value {
        b'.' | b' ' => None,
        b'A' => Some(ChangeKind::Added),
        b'M' => Some(ChangeKind::Modified),
        b'D' => Some(ChangeKind::Deleted),
        b'R' => Some(ChangeKind::Renamed),
        b'C' => Some(ChangeKind::Copied),
        b'T' => Some(ChangeKind::TypeChanged),
        b'U' => Some(ChangeKind::Unmerged),
        b'?' => Some(ChangeKind::Untracked),
        b'!' => Some(ChangeKind::Ignored),
        _ => Some(ChangeKind::Modified),
    }
}

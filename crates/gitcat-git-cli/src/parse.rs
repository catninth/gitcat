use std::collections::{HashMap, HashSet};

use gitcat_contracts::{
    ApiError, ApiResult, BranchInfo, ChangeKind, ChangedFile, CommitDetails, CommitSearchHit,
    CommitSummary, CommitTime, ConflictKind, DiffHunk, DiffLine, DiffLineKind, DiffStats, FileDiff,
    GraphCell, HeadState, Identity, LineStats, RefKind, RefLabel, StashEntry, StashRef,
    StatusEntry, WorktreeStatus,
};

use gitcat_contracts::ErrorCode;

pub(crate) const REF_FORMAT: &str = "%(refname)%00%(refname:short)%00%(objectname)%00%(*objectname)%00%(HEAD)%00%(upstream:short)%00%(upstream:track)%00%(symref)%00%(objecttype)";
// Git pretty formats have no length-prefixed field mode. NUL/RS are robust for normal commits,
// but a deliberately forged message containing these control bytes remains a documented P2 case.
pub(crate) const LOG_FORMAT: &str =
    "%x1e%H%x00%h%x00%P%x00%an%x00%ae%x00%at%x00%ai%x00%ct%x00%ci%x00%s%x00%b%x00";
pub(crate) const DETAIL_FORMAT: &str = "%x1e%H%x00%h%x00%T%x00%P%x00%an%x00%ae%x00%at%x00%ai%x00%cn%x00%ce%x00%ct%x00%ci%x00%s%x00%b%x00";
pub(crate) const STASH_GRAPH_FORMAT: &str = "%gd%x00%H%x00%P%x00%gs%x00";

#[derive(Debug)]
pub(crate) struct ParsedStatus {
    pub head: HeadState,
    pub status: WorktreeStatus,
}

#[derive(Debug, Clone)]
pub(crate) struct ParsedRef {
    pub oid: String,
    pub branch: Option<BranchInfo>,
    pub label: RefLabel,
    pub symbolic_target: Option<String>,
}

#[derive(Debug)]
pub(crate) struct ParsedCommitDetails {
    pub details: CommitDetails,
}

#[derive(Debug, Clone)]
pub(crate) struct StashCommit {
    pub reference: StashRef,
    pub label: String,
    pub stash_oid: String,
    pub base_oid: Option<String>,
}

#[derive(Debug, Default)]
pub(crate) struct StashGraph {
    pub commits: HashMap<String, StashCommit>,
    pub hidden: HashSet<String>,
    pub outer_to_visible: HashMap<String, String>,
}

impl StashGraph {
    pub fn is_empty(&self) -> bool {
        self.commits.is_empty() && self.hidden.is_empty()
    }

    pub fn entries(&self) -> Vec<StashEntry> {
        let mut entries = self
            .commits
            .values()
            .map(|stash| StashEntry {
                index: stash.reference.index,
                oid: stash.stash_oid.clone(),
                message: stash.label.clone(),
            })
            .collect::<Vec<_>>();
        entries.sort_unstable_by_key(|stash| stash.index);
        entries
    }
}

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

fn parse_ordinary_status(record: &[u8]) -> ApiResult<StatusEntry> {
    let fields: Vec<&[u8]> = record.splitn(9, |byte| *byte == b' ').collect();
    if fields.len() != 9 {
        return Err(parse_error("Malformed ordinary porcelain v2 status record"));
    }
    status_entry(fields[1], fields[2], fields[8], false)
}

fn parse_renamed_status(record: &[u8]) -> ApiResult<StatusEntry> {
    let fields: Vec<&[u8]> = record.splitn(10, |byte| *byte == b' ').collect();
    if fields.len() != 10 {
        return Err(parse_error("Malformed renamed porcelain v2 status record"));
    }
    status_entry(fields[1], fields[2], fields[9], false)
}

fn parse_unmerged_status(record: &[u8]) -> ApiResult<StatusEntry> {
    let fields: Vec<&[u8]> = record.splitn(11, |byte| *byte == b' ').collect();
    if fields.len() != 11 {
        return Err(parse_error("Malformed unmerged porcelain v2 status record"));
    }
    status_entry(fields[1], fields[2], fields[10], true)
}

fn status_entry(
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

fn change_from_status(value: u8) -> Option<ChangeKind> {
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

pub(crate) fn parse_refs(output: &[u8]) -> ApiResult<Vec<ParsedRef>> {
    let mut refs = Vec::new();
    for line in output.split(|byte| *byte == b'\n') {
        let line = line.strip_suffix(b"\r").unwrap_or(line);
        if line.is_empty() {
            continue;
        }
        let fields: Vec<&[u8]> = line.split(|byte| *byte == 0).collect();
        if fields.len() < 9 {
            return Err(parse_error("Malformed for-each-ref record"));
        }
        let full_name = text(fields[0]).into_owned();
        let name = text(fields[1]).into_owned();
        let direct_oid = text(fields[2]).into_owned();
        let peeled_oid = text(fields[3]).into_owned();
        let oid = if peeled_oid.is_empty() {
            direct_oid
        } else {
            peeled_oid
        };
        let is_head = fields[4] == b"*";
        let upstream = nonempty(fields[5]);
        let (ahead, behind) = parse_track(fields[6]);
        let symbolic_target = nonempty(fields[7]);
        let kind = if full_name.starts_with("refs/heads/") {
            RefKind::LocalBranch
        } else if full_name.starts_with("refs/remotes/") {
            RefKind::RemoteBranch
        } else if full_name.starts_with("refs/tags/") {
            RefKind::Tag
        } else {
            continue;
        };
        let label = RefLabel {
            name: name.clone(),
            full_name: full_name.clone(),
            kind,
            is_head,
        };
        let branch = match kind {
            RefKind::LocalBranch | RefKind::RemoteBranch => Some(BranchInfo {
                name,
                full_name,
                oid: oid.clone(),
                kind,
                is_head,
                upstream,
                ahead,
                behind,
            }),
            RefKind::Tag => None,
        };
        refs.push(ParsedRef {
            oid,
            branch,
            label,
            symbolic_target,
        });
    }
    Ok(refs)
}

fn parse_track(value: &[u8]) -> (Option<u32>, Option<u32>) {
    let value = text(value);
    let mut ahead = None;
    let mut behind = None;
    let words: Vec<&str> = value
        .trim_matches(['[', ']'])
        .split([',', ' '])
        .filter(|word| !word.is_empty())
        .collect();
    for pair in words.windows(2) {
        match pair[0] {
            "ahead" => ahead = pair[1].parse().ok(),
            "behind" => behind = pair[1].parse().ok(),
            _ => {}
        }
    }
    (ahead, behind)
}

pub(crate) fn parse_log(output: &[u8]) -> ApiResult<Vec<CommitSummary>> {
    let mut commits = Vec::new();
    for record in output.split(|byte| *byte == 0x1e) {
        if record
            .iter()
            .all(|byte| byte.is_ascii_whitespace() || *byte == 0)
        {
            continue;
        }
        let record = trim_record(record);
        let fields: Vec<&[u8]> = record.split(|byte| *byte == 0).collect();
        if fields.len() < 11 {
            return Err(parse_error("Malformed Git log record"));
        }
        let subject = text(fields[9]).into_owned();
        let body = text(fields[10]).trim().to_owned();
        commits.push(CommitSummary {
            oid: text(fields[0]).into_owned(),
            short_oid: text(fields[1]).into_owned(),
            parent_oids: split_oids(fields[2]),
            subject,
            body_preview: preview(&body, 240),
            author: Identity {
                name: text(fields[3]).into_owned(),
                email: text(fields[4]).into_owned(),
            },
            authored_at: parse_time(fields[5], fields[6])?,
            committed_at: parse_time(fields[7], fields[8])?,
            decorations: Vec::new(),
            stash: None,
            graph: GraphCell::default(),
        });
    }
    Ok(commits)
}

pub(crate) fn parse_search_hits(output: &[u8], query: &str) -> ApiResult<Vec<CommitSearchHit>> {
    let query_lower = query.to_lowercase();
    let mut hits = Vec::new();
    for record in output.split(|byte| *byte == 0x1e) {
        if record
            .iter()
            .all(|byte| byte.is_ascii_whitespace() || *byte == 0)
        {
            continue;
        }
        let fields: Vec<&[u8]> = trim_record(record).split(|byte| *byte == 0).collect();
        if fields.len() < 11 {
            return Err(parse_error("Malformed Git search record"));
        }
        let subject = text(fields[9]).into_owned();
        let body = text(fields[10]).trim().to_owned();
        let subject_lower = subject.to_lowercase();
        let body_lower = body.to_lowercase();
        let matched_subject = subject_lower.contains(&query_lower);
        let matched_body = body_lower.contains(&query_lower);
        hits.push(CommitSearchHit {
            oid: text(fields[0]).into_owned(),
            subject,
            body_excerpt: matched_body.then(|| excerpt_around(&body, &query_lower, 180)),
            matched_subject,
            matched_body,
        });
    }
    Ok(hits)
}

pub(crate) fn parse_commit_details(output: &[u8]) -> ApiResult<ParsedCommitDetails> {
    let record = output
        .split(|byte| *byte == 0x1e)
        .find(|record| {
            !record
                .iter()
                .all(|byte| byte.is_ascii_whitespace() || *byte == 0)
        })
        .ok_or_else(|| parse_error("Git did not return commit details"))?;
    let fields: Vec<&[u8]> = trim_record(record).split(|byte| *byte == 0).collect();
    if fields.len() < 14 {
        return Err(parse_error("Malformed commit detail record"));
    }
    Ok(ParsedCommitDetails {
        details: CommitDetails {
            oid: text(fields[0]).into_owned(),
            short_oid: text(fields[1]).into_owned(),
            tree_oid: text(fields[2]).into_owned(),
            parent_oids: split_oids(fields[3]),
            author: Identity {
                name: text(fields[4]).into_owned(),
                email: text(fields[5]).into_owned(),
            },
            authored_at: parse_time(fields[6], fields[7])?,
            committer: Identity {
                name: text(fields[8]).into_owned(),
                email: text(fields[9]).into_owned(),
            },
            committed_at: parse_time(fields[10], fields[11])?,
            subject: text(fields[12]).into_owned(),
            body: text(fields[13]).trim().to_owned(),
            stats: DiffStats {
                files: 0,
                additions: 0,
                deletions: 0,
            },
            files: Vec::new(),
        },
    })
}

fn parse_time(seconds: &[u8], formatted: &[u8]) -> ApiResult<CommitTime> {
    let seconds = text(seconds)
        .trim()
        .parse::<i64>()
        .map_err(|_| parse_error("Git timestamp was malformed"))?;
    let formatted = text(formatted);
    let offset = formatted.split_whitespace().last().unwrap_or("+0000");
    let offset_minutes = parse_offset(offset).unwrap_or(0);
    Ok(CommitTime {
        seconds,
        offset_minutes,
    })
}

fn parse_offset(value: &str) -> Option<i16> {
    if value.len() != 5 {
        return None;
    }
    let sign = match &value[..1] {
        "+" => 1,
        "-" => -1,
        _ => return None,
    };
    let hours: i16 = value[1..3].parse().ok()?;
    let minutes: i16 = value[3..5].parse().ok()?;
    Some(sign * (hours * 60 + minutes))
}

pub(crate) fn parse_changed_files(
    name_status: &[u8],
    numstat: &[u8],
) -> ApiResult<Vec<ChangedFile>> {
    let mut files = parse_name_status(name_status)?;
    let stats = parse_numstat(numstat)?;
    for file in &mut files {
        if let Some((additions, deletions, binary)) = stats.get(&file.new_path) {
            file.additions = *additions;
            file.deletions = *deletions;
            file.binary = *binary;
        }
    }
    Ok(files)
}

pub(crate) fn parse_name_status(output: &[u8]) -> ApiResult<Vec<ChangedFile>> {
    let tokens: Vec<&[u8]> = output
        .split(|byte| *byte == 0)
        .filter(|part| !part.is_empty())
        .collect();
    let mut files = Vec::new();
    let mut index = 0;
    while index < tokens.len() {
        let token = trim_line(tokens[index]);
        index += 1;
        if token.is_empty() {
            continue;
        }
        let (status_token, inline_path) =
            split_once(token, b'\t').map_or((token, None), |(status, path)| (status, Some(path)));
        let (status, similarity, path_count) = parse_file_status(status_token)?;
        let first_path = if let Some(path) = inline_path.filter(|path| !path.is_empty()) {
            path
        } else {
            let path = *tokens
                .get(index)
                .ok_or_else(|| parse_error("Name-status path missing"))?;
            index += 1;
            path
        };
        let (old_path, new_path) = if path_count == 2 {
            let second_path = if inline_path.is_some() {
                split_once(first_path, b'\t')
                    .map(|(_, path)| path)
                    .unwrap_or_else(|| {
                        let value = tokens.get(index).copied().unwrap_or_default();
                        index += usize::from(index < tokens.len());
                        value
                    })
            } else {
                let value = *tokens
                    .get(index)
                    .ok_or_else(|| parse_error("Rename destination path missing"))?;
                index += 1;
                value
            };
            (
                Some(text(first_path).into_owned()),
                text(second_path).into_owned(),
            )
        } else {
            (None, text(first_path).into_owned())
        };
        files.push(ChangedFile {
            old_path,
            new_path,
            status,
            additions: None,
            deletions: None,
            similarity,
            binary: false,
        });
    }
    Ok(files)
}

fn parse_file_status(value: &[u8]) -> ApiResult<(ChangeKind, Option<u8>, usize)> {
    let code = *value
        .first()
        .ok_or_else(|| parse_error("Empty file status"))?;
    let status = match code {
        b'A' => ChangeKind::Added,
        b'M' => ChangeKind::Modified,
        b'D' => ChangeKind::Deleted,
        b'R' => ChangeKind::Renamed,
        b'C' => ChangeKind::Copied,
        b'T' => ChangeKind::TypeChanged,
        b'U' => ChangeKind::Unmerged,
        _ => return Err(parse_error("Unknown file status")),
    };
    let similarity = if matches!(code, b'R' | b'C') {
        text(&value[1..]).parse().ok()
    } else {
        None
    };
    Ok((
        status,
        similarity,
        usize::from(matches!(code, b'R' | b'C')) + 1,
    ))
}

type FileStats = (Option<u64>, Option<u64>, bool);

pub(crate) fn parse_line_stats(output: &[u8]) -> ApiResult<HashMap<String, LineStats>> {
    let stats = parse_numstat(output)?;
    Ok(stats
        .into_iter()
        .filter_map(|(path, (additions, deletions, binary))| {
            if binary {
                return None;
            }
            Some((
                path,
                LineStats {
                    additions: additions?,
                    deletions: deletions?,
                },
            ))
        })
        .collect())
}

fn parse_numstat(output: &[u8]) -> ApiResult<HashMap<String, FileStats>> {
    let tokens: Vec<&[u8]> = output
        .split(|byte| *byte == 0)
        .filter(|part| !part.is_empty())
        .collect();
    let mut result = HashMap::new();
    let mut index = 0;
    while index < tokens.len() {
        let token = trim_line(tokens[index]);
        index += 1;
        let mut fields = token.splitn(3, |byte| *byte == b'\t');
        let additions = fields
            .next()
            .ok_or_else(|| parse_error("Numstat additions missing"))?;
        let deletions = fields
            .next()
            .ok_or_else(|| parse_error("Numstat deletions missing"))?;
        let path = fields
            .next()
            .ok_or_else(|| parse_error("Numstat path missing"))?;
        let binary = additions == b"-" || deletions == b"-";
        let additions = (!binary).then(|| text(additions).parse().ok()).flatten();
        let deletions = (!binary).then(|| text(deletions).parse().ok()).flatten();
        let final_path = if path.is_empty() {
            let _old = tokens
                .get(index)
                .ok_or_else(|| parse_error("Numstat rename source missing"))?;
            index += 1;
            let new = tokens
                .get(index)
                .ok_or_else(|| parse_error("Numstat rename destination missing"))?;
            index += 1;
            text(new).into_owned()
        } else {
            text(path).into_owned()
        };
        result.insert(final_path, (additions, deletions, binary));
    }
    Ok(result)
}

pub(crate) fn parse_file_diff(
    output: &[u8],
    requested_path: &str,
    truncated: bool,
) -> ApiResult<FileDiff> {
    let content = text(output);
    if content
        .lines()
        .filter(|line| line.starts_with("diff --git "))
        .take(2)
        .count()
        > 1
    {
        return Err(ApiError::new(
            ErrorCode::InvalidRequest,
            "Diff request must select exactly one file",
        ));
    }
    let mut old_path = None;
    let mut new_path = requested_path.to_owned();
    let mut old_mode = None;
    let mut new_mode = None;
    let mut status = ChangeKind::Modified;
    let mut binary = false;
    let mut hunks = Vec::new();
    let mut current: Option<DiffHunk> = None;
    let mut old_line = 0_u32;
    let mut new_line = 0_u32;
    let mut additions = 0_u64;
    let mut deletions = 0_u64;

    for line in content.lines() {
        if let Some(value) = line.strip_prefix("old mode ") {
            old_mode = Some(value.to_owned());
        } else if let Some(value) = line.strip_prefix("new mode ") {
            new_mode = Some(value.to_owned());
            status = ChangeKind::TypeChanged;
        } else if let Some(value) = line.strip_prefix("new file mode ") {
            new_mode = Some(value.to_owned());
            status = ChangeKind::Added;
        } else if let Some(value) = line.strip_prefix("deleted file mode ") {
            old_mode = Some(value.to_owned());
            status = ChangeKind::Deleted;
        } else if let Some(value) = line.strip_prefix("rename from ") {
            old_path = Some(value.to_owned());
            status = ChangeKind::Renamed;
        } else if let Some(value) = line.strip_prefix("rename to ") {
            new_path = value.to_owned();
        } else if line.starts_with("Binary files ") || line == "GIT binary patch" {
            binary = true;
        } else if line.starts_with("@@ ") {
            if let Some(hunk) = current.take() {
                hunks.push(hunk);
            }
            let (old_start, old_count, new_start, new_count) = parse_hunk_header(line)?;
            old_line = old_start;
            new_line = new_start;
            current = Some(DiffHunk {
                header: line.to_owned(),
                old_start,
                old_count,
                new_start,
                new_count,
                lines: Vec::new(),
            });
        } else if let Some(hunk) = current.as_mut() {
            let (kind, old_number, new_number, body) = if let Some(body) = line.strip_prefix('+') {
                let number = new_line;
                new_line = new_line.saturating_add(1);
                additions += 1;
                (DiffLineKind::Addition, None, Some(number), body)
            } else if let Some(body) = line.strip_prefix('-') {
                let number = old_line;
                old_line = old_line.saturating_add(1);
                deletions += 1;
                (DiffLineKind::Deletion, Some(number), None, body)
            } else if let Some(body) = line.strip_prefix(' ') {
                let old_number = old_line;
                let new_number = new_line;
                old_line = old_line.saturating_add(1);
                new_line = new_line.saturating_add(1);
                (
                    DiffLineKind::Context,
                    Some(old_number),
                    Some(new_number),
                    body,
                )
            } else if line.starts_with('\\') {
                (DiffLineKind::NoNewline, None, None, line)
            } else {
                continue;
            };
            hunk.lines.push(DiffLine {
                kind,
                old_line: old_number,
                new_line: new_number,
                content: body.to_owned(),
            });
        }
    }
    if let Some(hunk) = current {
        hunks.push(hunk);
    }
    Ok(FileDiff {
        old_path,
        new_path,
        old_mode,
        new_mode,
        status,
        binary,
        stats: DiffStats {
            files: u32::from(!output.is_empty()),
            additions,
            deletions,
        },
        hunks,
        truncated,
    })
}

fn parse_hunk_header(header: &str) -> ApiResult<(u32, u32, u32, u32)> {
    let end = header[3..]
        .find(" @@")
        .map(|index| index + 3)
        .ok_or_else(|| parse_error("Diff hunk header was malformed"))?;
    let ranges = &header[3..end];
    let mut parts = ranges.split_whitespace();
    let old = parts
        .next()
        .ok_or_else(|| parse_error("Old hunk range missing"))?;
    let new = parts
        .next()
        .ok_or_else(|| parse_error("New hunk range missing"))?;
    let (old_start, old_count) = parse_range(old, '-')?;
    let (new_start, new_count) = parse_range(new, '+')?;
    Ok((old_start, old_count, new_start, new_count))
}

fn parse_range(value: &str, prefix: char) -> ApiResult<(u32, u32)> {
    let value = value
        .strip_prefix(prefix)
        .ok_or_else(|| parse_error("Diff hunk range prefix was malformed"))?;
    let (start, count) = value.split_once(',').unwrap_or((value, "1"));
    Ok((
        start
            .parse()
            .map_err(|_| parse_error("Diff hunk start was malformed"))?,
        count
            .parse()
            .map_err(|_| parse_error("Diff hunk count was malformed"))?,
    ))
}

pub(crate) fn parse_stash_graph(output: &[u8]) -> ApiResult<StashGraph> {
    let mut graph = StashGraph::default();
    for line in output.split(|byte| *byte == b'\n') {
        let fields: Vec<&[u8]> = trim_line(line).split(|byte| *byte == 0).collect();
        if fields.first().is_none_or(|field| field.is_empty()) {
            continue;
        }
        if fields.len() < 4 {
            return Err(parse_error("Malformed stash graph record"));
        }
        let selector = text(fields[0]).into_owned();
        let index = selector
            .strip_prefix("stash@{")
            .and_then(|value| value.strip_suffix('}'))
            .and_then(|value| value.parse().ok())
            .ok_or_else(|| parse_error("Malformed stash selector"))?;
        let stash_oid = text(fields[1]).into_owned();
        let parents = split_oids(fields[2]);
        let base_oid = parents.first().cloned();
        let visible_oid = parents.get(1).cloned().unwrap_or_else(|| stash_oid.clone());
        let label = stash_label(&text(fields[3]));

        if visible_oid != stash_oid {
            graph.hidden.insert(stash_oid.clone());
            graph.hidden.extend(
                parents
                    .iter()
                    .skip(2)
                    .filter(|oid| {
                        oid.as_str() != visible_oid.as_str()
                            && Some(oid.as_str()) != base_oid.as_deref()
                    })
                    .cloned(),
            );
        }
        graph
            .outer_to_visible
            .insert(stash_oid.clone(), visible_oid.clone());

        if let Some(existing) = graph.commits.get_mut(&visible_oid) {
            // `git stash list` is newest-first. GitKraken collapses stash
            // entries that share the same visible index-parent commit: the
            // oldest member supplies the displayed label, while actions and
            // diffs continue to target the newest member inserted first.
            existing.label = label;
        } else {
            graph.commits.insert(
                visible_oid,
                StashCommit {
                    reference: StashRef {
                        index,
                        selector,
                        oid: stash_oid.clone(),
                    },
                    label,
                    stash_oid,
                    base_oid,
                },
            );
        }
    }
    Ok(graph)
}

fn stash_label(message: &str) -> String {
    let message = message.trim();
    if let Some(rest) = message.strip_prefix("On ") {
        if let Some((_, described)) = rest.split_once(": ") {
            let described = described.trim();
            if !described.is_empty() {
                return described.to_owned();
            }
        }
    }
    if let Some(rest) = message.strip_prefix("WIP on ") {
        if let Some((branch, _)) = rest.split_once(": ") {
            let branch = branch.trim();
            if !branch.is_empty() {
                return format!("WIP on {branch}");
            }
        }
    }
    message.to_owned()
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
}

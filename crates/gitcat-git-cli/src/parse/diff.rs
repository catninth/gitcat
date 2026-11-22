//! Changed files, line stats and structured file diffs.

use super::*;

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

pub(super) fn parse_file_status(value: &[u8]) -> ApiResult<(ChangeKind, Option<u8>, usize)> {
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

pub(super) type FileStats = (Option<u64>, Option<u64>, bool);

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

pub(super) fn parse_numstat(output: &[u8]) -> ApiResult<HashMap<String, FileStats>> {
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

pub(super) fn parse_hunk_header(header: &str) -> ApiResult<(u32, u32, u32, u32)> {
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

pub(super) fn parse_range(value: &str, prefix: char) -> ApiResult<(u32, u32)> {
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

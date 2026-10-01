//! Builds a patch that carries only the chosen lines of a one-file diff.
//!
//! The patch is cut from Git's raw output rather than from the parsed
//! `FileDiff`, so line endings and bytes that are not UTF-8 reach `git apply`
//! exactly as Git wrote them.

use std::collections::HashMap;

use gitcat_contracts::{ApiError, ApiResult, DiffLine, DiffLineKind, ErrorCode};

/// Which way the patch will be applied. A forward patch adds its chosen lines
/// to the old side (staging); a reverse one takes them back out of the new
/// side (unstaging, discarding).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PatchDirection {
    Forward,
    Reverse,
}

struct Hunk<'a> {
    old_start: u32,
    old_count: u32,
    new_start: u32,
    new_count: u32,
    lines: Vec<&'a [u8]>,
}

pub(crate) fn build_line_patch(
    raw: &[u8],
    path: &str,
    selected: &[DiffLine],
    direction: PatchDirection,
) -> ApiResult<Vec<u8>> {
    let mut additions: HashMap<u32, &str> = HashMap::new();
    let mut deletions: HashMap<u32, &str> = HashMap::new();
    for line in selected {
        let (map, number) = match line.kind {
            DiffLineKind::Addition => (&mut additions, line.new_line),
            DiffLineKind::Deletion => (&mut deletions, line.old_line),
            DiffLineKind::Context | DiffLineKind::NoNewline => continue,
        };
        let number = number.ok_or_else(|| {
            ApiError::new(
                ErrorCode::InvalidRequest,
                "A changed line has no line number",
            )
        })?;
        map.insert(number, line.content.as_str());
    }
    let wanted = additions.len() + deletions.len();
    if wanted == 0 {
        return Err(ApiError::new(
            ErrorCode::InvalidRequest,
            "Choose at least one added or removed line",
        ));
    }

    let mut out = file_header(path);
    let mut matched = 0;
    // Lines this patch adds minus lines it removes, over the hunks already
    // written: it moves where the next hunk starts on the side that changes.
    let mut shift: i64 = 0;

    for hunk in parse_hunks(raw)? {
        let mut body: Vec<u8> = Vec::new();
        let mut old_count = 0_u32;
        let mut new_count = 0_u32;
        let mut picked = false;
        let mut previous_kept = false;
        let mut old_line = hunk.old_start.max(1);
        let mut new_line = hunk.new_start.max(1);

        for line in &hunk.lines {
            let (prefix, text) = match line.split_first() {
                Some((prefix, text)) => (*prefix, text),
                // `diff.suppressBlankEmpty` writes an empty context line bare.
                None => (b' ', &[][..]),
            };
            let emitted = match prefix {
                b' ' => {
                    old_line += 1;
                    new_line += 1;
                    Some(b' ')
                }
                b'+' => {
                    let chosen = take(&additions, new_line, text, &mut matched)?;
                    new_line += 1;
                    picked |= chosen;
                    match (chosen, direction) {
                        (true, _) => Some(b'+'),
                        (false, PatchDirection::Forward) => None,
                        (false, PatchDirection::Reverse) => Some(b' '),
                    }
                }
                b'-' => {
                    let chosen = take(&deletions, old_line, text, &mut matched)?;
                    old_line += 1;
                    picked |= chosen;
                    match (chosen, direction) {
                        (true, _) => Some(b'-'),
                        (false, PatchDirection::Forward) => Some(b' '),
                        (false, PatchDirection::Reverse) => None,
                    }
                }
                // The marker belongs to the line before it and goes wherever
                // that line went.
                b'\\' => {
                    if previous_kept {
                        body.extend_from_slice(line);
                        body.push(b'\n');
                    }
                    continue;
                }
                _ => continue,
            };
            previous_kept = emitted.is_some();
            if let Some(prefix) = emitted {
                if prefix != b'+' {
                    old_count += 1;
                }
                if prefix != b'-' {
                    new_count += 1;
                }
                body.push(prefix);
                body.extend_from_slice(text);
                body.push(b'\n');
            }
        }

        if !picked {
            continue;
        }
        let (old_start, new_start) = match direction {
            PatchDirection::Forward => {
                let first_old = first_line(hunk.old_start, hunk.old_count);
                (hunk.old_start, start_of(first_old + shift, new_count))
            }
            PatchDirection::Reverse => {
                let first_new = first_line(hunk.new_start, hunk.new_count);
                (start_of(first_new - shift, old_count), hunk.new_start)
            }
        };
        shift += i64::from(new_count) - i64::from(old_count);
        out.extend_from_slice(
            format!("@@ -{old_start},{old_count} +{new_start},{new_count} @@\n").as_bytes(),
        );
        out.extend_from_slice(&body);
    }

    if matched != wanted {
        return Err(stale());
    }
    Ok(out)
}

fn take(
    chosen: &HashMap<u32, &str>,
    number: u32,
    text: &[u8],
    matched: &mut usize,
) -> ApiResult<bool> {
    let Some(expected) = chosen.get(&number) else {
        return Ok(false);
    };
    let actual = String::from_utf8_lossy(text);
    if actual.trim_end_matches('\r') != expected.trim_end_matches('\r') {
        return Err(stale());
    }
    *matched += 1;
    Ok(true)
}

fn stale() -> ApiError {
    ApiError::new(
        ErrorCode::StaleSnapshot,
        "The file changed since its diff was shown; reload the diff and try again",
    )
}

// A hunk header names the line before an empty range, and the first line of
// a range that has content.
fn first_line(start: u32, count: u32) -> i64 {
    if count == 0 {
        i64::from(start) + 1
    } else {
        i64::from(start)
    }
}

fn start_of(first: i64, count: u32) -> u32 {
    let start = if count == 0 { first - 1 } else { first };
    u32::try_from(start.max(0)).unwrap_or(0)
}

fn file_header(path: &str) -> Vec<u8> {
    let path = quote_path(path);
    format!("diff --git a/{path} b/{path}\n--- a/{path}\n+++ b/{path}\n").into_bytes()
}

// Git's own C-style quoting for names it would not write bare. Non-ASCII is
// left as UTF-8, which `git apply` reads either way.
fn quote_path(path: &str) -> String {
    let needs = path
        .bytes()
        .any(|byte| byte < 0x20 || byte == 0x7f || byte == b'"' || byte == b'\\');
    if !needs {
        return path.to_owned();
    }
    let mut quoted = String::from("\"");
    for character in path.chars() {
        match character {
            '"' => quoted.push_str("\\\""),
            '\\' => quoted.push_str("\\\\"),
            '\t' => quoted.push_str("\\t"),
            '\n' => quoted.push_str("\\n"),
            character if (character as u32) < 0x20 || character as u32 == 0x7f => {
                quoted.push_str(&format!("\\{:03o}", character as u32));
            }
            character => quoted.push(character),
        }
    }
    quoted.push('"');
    quoted
}

fn parse_hunks(raw: &[u8]) -> ApiResult<Vec<Hunk<'_>>> {
    let mut lines = raw.split(|byte| *byte == b'\n').peekable();
    let mut hunks = Vec::new();
    while let Some(line) = lines.next() {
        if !line.starts_with(b"@@ ") {
            continue;
        }
        let (old_start, old_count, new_start, new_count) =
            parse_header(&String::from_utf8_lossy(line))?;
        let mut hunk = Hunk {
            old_start,
            old_count,
            new_start,
            new_count,
            lines: Vec::new(),
        };
        // Counting rather than looking for the next header is what makes a
        // bare empty line readable as context.
        let mut old_left = old_count;
        let mut new_left = new_count;
        while old_left > 0 || new_left > 0 {
            let Some(line) = lines.next() else {
                return Err(malformed());
            };
            match line.first() {
                Some(b'+') => new_left = new_left.checked_sub(1).ok_or_else(malformed)?,
                Some(b'-') => old_left = old_left.checked_sub(1).ok_or_else(malformed)?,
                Some(b'\\') => {}
                _ => {
                    old_left = old_left.checked_sub(1).ok_or_else(malformed)?;
                    new_left = new_left.checked_sub(1).ok_or_else(malformed)?;
                }
            }
            hunk.lines.push(line);
        }
        while let Some(line) = lines.next_if(|line| line.starts_with(b"\\")) {
            hunk.lines.push(line);
        }
        hunks.push(hunk);
    }
    Ok(hunks)
}

fn parse_header(header: &str) -> ApiResult<(u32, u32, u32, u32)> {
    let ranges = header
        .strip_prefix("@@ ")
        .and_then(|rest| rest.split(" @@").next())
        .ok_or_else(malformed)?;
    let mut parts = ranges.split_whitespace();
    let old = parts.next().and_then(|part| part.strip_prefix('-'));
    let new = parts.next().and_then(|part| part.strip_prefix('+'));
    let (Some(old), Some(new)) = (old, new) else {
        return Err(malformed());
    };
    let (old_start, old_count) = parse_range(old)?;
    let (new_start, new_count) = parse_range(new)?;
    Ok((old_start, old_count, new_start, new_count))
}

fn parse_range(value: &str) -> ApiResult<(u32, u32)> {
    let (start, count) = value.split_once(',').unwrap_or((value, "1"));
    Ok((
        start.parse().map_err(|_| malformed())?,
        count.parse().map_err(|_| malformed())?,
    ))
}

fn malformed() -> ApiError {
    ApiError::new(ErrorCode::GitCommandFailed, "Diff hunk was malformed")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(
        kind: DiffLineKind,
        old_line: Option<u32>,
        new_line: Option<u32>,
        content: &str,
    ) -> DiffLine {
        DiffLine {
            kind,
            old_line,
            new_line,
            content: content.to_owned(),
        }
    }

    const RAW: &str = concat!(
        "diff --git a/f.txt b/f.txt\n",
        "index 1..2 100644\n",
        "--- a/f.txt\n",
        "+++ b/f.txt\n",
        "@@ -1,4 +1,4 @@\n",
        " one\n",
        "-two\n",
        "-three\n",
        "+TWO\n",
        "+THREE\n",
        " four\n",
        "@@ -10,2 +10,3 @@\n",
        " ten\n",
        "+inserted\n",
        " eleven\n",
    );

    fn text(patch: Vec<u8>) -> String {
        String::from_utf8(patch).unwrap()
    }

    #[test]
    fn forward_keeps_unchosen_deletions_as_context_and_drops_unchosen_additions() {
        let patch = build_line_patch(
            RAW.as_bytes(),
            "f.txt",
            &[
                line(DiffLineKind::Deletion, Some(2), None, "two"),
                line(DiffLineKind::Addition, None, Some(2), "TWO"),
            ],
            PatchDirection::Forward,
        )
        .unwrap();
        assert_eq!(
            text(patch),
            concat!(
                "diff --git a/f.txt b/f.txt\n--- a/f.txt\n+++ b/f.txt\n",
                "@@ -1,4 +1,4 @@\n one\n-two\n three\n+TWO\n four\n",
            )
        );
    }

    #[test]
    fn reverse_keeps_unchosen_additions_as_context_and_drops_unchosen_deletions() {
        let patch = build_line_patch(
            RAW.as_bytes(),
            "f.txt",
            &[line(DiffLineKind::Addition, None, Some(3), "THREE")],
            PatchDirection::Reverse,
        )
        .unwrap();
        assert_eq!(
            text(patch),
            concat!(
                "diff --git a/f.txt b/f.txt\n--- a/f.txt\n+++ b/f.txt\n",
                "@@ -1,3 +1,4 @@\n one\n TWO\n+THREE\n four\n",
            )
        );
    }

    #[test]
    fn later_hunks_shift_by_what_earlier_hunks_change() {
        let patch = build_line_patch(
            RAW.as_bytes(),
            "f.txt",
            &[
                line(DiffLineKind::Deletion, Some(3), None, "three"),
                line(DiffLineKind::Addition, None, Some(11), "inserted"),
            ],
            PatchDirection::Forward,
        )
        .unwrap();
        assert!(text(patch).ends_with("@@ -10,2 +9,3 @@\n ten\n+inserted\n eleven\n"));
    }

    #[test]
    fn a_line_that_no_longer_matches_is_refused() {
        let error = build_line_patch(
            RAW.as_bytes(),
            "f.txt",
            &[line(
                DiffLineKind::Addition,
                None,
                Some(2),
                "something else",
            )],
            PatchDirection::Forward,
        )
        .unwrap_err();
        assert_eq!(error.code, ErrorCode::StaleSnapshot);

        let error = build_line_patch(
            RAW.as_bytes(),
            "f.txt",
            &[line(DiffLineKind::Addition, None, Some(40), "TWO")],
            PatchDirection::Forward,
        )
        .unwrap_err();
        assert_eq!(error.code, ErrorCode::StaleSnapshot);
    }

    #[test]
    fn line_endings_and_missing_newline_markers_survive() {
        let raw = "@@ -1,2 +1,2 @@\n a\r\n-b\r\n\\ No newline at end of file\n+c\r\n\\ No newline at end of file\n";
        let patch = build_line_patch(
            raw.as_bytes(),
            "f.txt",
            &[
                line(DiffLineKind::Deletion, Some(2), None, "b"),
                line(DiffLineKind::Addition, None, Some(2), "c"),
            ],
            PatchDirection::Forward,
        )
        .unwrap();
        assert!(text(patch).ends_with(
            "@@ -1,2 +1,2 @@\n a\r\n-b\r\n\\ No newline at end of file\n+c\r\n\\ No newline at end of file\n"
        ));
    }

    #[test]
    fn an_empty_old_side_starts_at_the_first_line_once_it_has_content() {
        let raw = "@@ -0,0 +1,3 @@\n+a\n+b\n+c\n";
        let patch = build_line_patch(
            raw.as_bytes(),
            "f.txt",
            &[line(DiffLineKind::Addition, None, Some(2), "b")],
            PatchDirection::Reverse,
        )
        .unwrap();
        assert!(text(patch).ends_with("@@ -1,2 +1,3 @@\n a\n+b\n c\n"));
    }

    #[test]
    fn bare_empty_lines_are_context() {
        let raw = "@@ -1,3 +1,3 @@\n a\n\n-b\n+c\n";
        let patch = build_line_patch(
            raw.as_bytes(),
            "f.txt",
            &[line(DiffLineKind::Addition, None, Some(3), "c")],
            PatchDirection::Forward,
        )
        .unwrap();
        assert!(text(patch).ends_with("@@ -1,3 +1,4 @@\n a\n \n b\n+c\n"));
    }

    #[test]
    fn awkward_names_are_quoted() {
        assert_eq!(quote_path("plain name.txt"), "plain name.txt");
        assert_eq!(quote_path("a\"b\\c"), "\"a\\\"b\\\\c\"");
    }
}

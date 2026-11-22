//! What a conflicted file holds: text or binary, its line endings, leftover markers.

use super::*;

/// `fd~HEAD` next to a directory `fd`: the copy Git writes aside when a file
/// and a directory claim the same name. Staging it as resolved would commit a
/// file nobody created.
pub(crate) fn is_helper_copy(root: &Path, relative: &str) -> bool {
    let Some((prefix, side)) = relative.rsplit_once('~') else {
        return false;
    };
    !prefix.is_empty() && !side.is_empty() && !side.contains('/') && root.join(prefix).is_dir()
}

/// A complete marker block left in the working copy. Only regular text files
/// within the editor's size limit are read; anything else cannot be checked
/// and is not reported.
pub(crate) fn worktree_has_conflict_markers(file: &Path) -> bool {
    let Ok(metadata) = fs::symlink_metadata(file) else {
        return false;
    };
    if !metadata.is_file() || metadata.len() > MAX_CONFLICT_TEXT_BYTES as u64 {
        return false;
    }
    let Ok(bytes) = fs::read(file) else {
        return false;
    };
    if bytes.contains(&0) {
        return false;
    }
    has_conflict_markers(&String::from_utf8_lossy(&bytes))
}

pub(crate) fn has_conflict_markers(text: &str) -> bool {
    let (mut open, mut middle) = (false, false);
    for line in text.lines() {
        let line = line.strip_suffix('\r').unwrap_or(line);
        if line.starts_with("<<<<<<<") {
            open = true;
        } else if open && line == "=======" {
            middle = true;
        } else if middle && line.starts_with(">>>>>>>") {
            return true;
        }
    }
    false
}

pub(crate) fn classify_conflict_content(bytes: Vec<u8>, reported_size: u64) -> ConflictFileContent {
    if bytes.len() > MAX_CONFLICT_TEXT_BYTES {
        return ConflictFileContent {
            kind: ConflictContentKind::TooLarge,
            size: Some(bytes.len() as u64),
            text: None,
            line_ending: None,
        };
    }
    if bytes.contains(&0) {
        return ConflictFileContent {
            kind: ConflictContentKind::Binary,
            size: Some(reported_size),
            text: None,
            line_ending: None,
        };
    }
    match String::from_utf8(bytes) {
        Ok(text) => {
            let line_ending = detect_line_ending(&text);
            ConflictFileContent {
                kind: ConflictContentKind::Text,
                size: Some(reported_size),
                text: Some(text),
                line_ending: Some(line_ending),
            }
        }
        Err(_) => ConflictFileContent {
            kind: ConflictContentKind::Binary,
            size: Some(reported_size),
            text: None,
            line_ending: None,
        },
    }
}

#[cfg(unix)]
pub(crate) fn os_string_bytes(value: &std::ffi::OsStr) -> Vec<u8> {
    use std::os::unix::ffi::OsStrExt;
    value.as_bytes().to_vec()
}

#[cfg(windows)]
pub(crate) fn os_string_bytes(value: &std::ffi::OsStr) -> Vec<u8> {
    use std::os::windows::ffi::OsStrExt;
    value
        .encode_wide()
        .flat_map(u16::to_le_bytes)
        .collect::<Vec<_>>()
}

#[cfg(not(any(unix, windows)))]
pub(crate) fn os_string_bytes(value: &std::ffi::OsStr) -> Vec<u8> {
    value.to_string_lossy().as_bytes().to_vec()
}

pub(crate) fn detect_line_ending(text: &str) -> ConflictLineEnding {
    let bytes = text.as_bytes();
    let mut saw_lf = false;
    let mut saw_crlf = false;
    let mut saw_bare_cr = false;
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'\r' if bytes.get(index + 1) == Some(&b'\n') => {
                saw_crlf = true;
                index += 2;
            }
            b'\r' => {
                saw_bare_cr = true;
                index += 1;
            }
            b'\n' => {
                saw_lf = true;
                index += 1;
            }
            _ => index += 1,
        }
    }
    match (saw_lf, saw_crlf, saw_bare_cr) {
        (false, false, false) => ConflictLineEnding::None,
        (true, false, false) => ConflictLineEnding::Lf,
        (false, true, false) => ConflictLineEnding::CrLf,
        _ => ConflictLineEnding::Mixed,
    }
}

pub(crate) fn normalize_line_endings(text: &str) -> String {
    text.replace("\r\n", "\n").replace('\r', "\n")
}

pub(crate) fn encode_edited_conflict_text(
    text: &str,
    policy: ConflictLineEndingPolicy,
    current: &ConflictFileContent,
) -> ApiResult<Vec<u8>> {
    match current.kind {
        ConflictContentKind::Missing | ConflictContentKind::Text => {
            let normalized = normalize_line_endings(text);
            match policy {
                ConflictLineEndingPolicy::Lf => Ok(normalized.into_bytes()),
                ConflictLineEndingPolicy::CrLf => Ok(normalized.replace('\n', "\r\n").into_bytes()),
                ConflictLineEndingPolicy::Preserve => {
                    match current.line_ending.unwrap_or(ConflictLineEnding::None) {
                        ConflictLineEnding::CrLf => {
                            Ok(normalized.replace('\n', "\r\n").into_bytes())
                        }
                        ConflictLineEnding::Lf | ConflictLineEnding::None => {
                            Ok(normalized.into_bytes())
                        }
                        ConflictLineEnding::Mixed => {
                            let original = current.text.as_deref().unwrap_or_default();
                            if normalize_line_endings(original) == normalized {
                                Ok(original.as_bytes().to_vec())
                            } else {
                                Err(ApiError::new(
                                    ErrorCode::InvalidRequest,
                                    "Edited mixed line endings require an explicit LF or CRLF policy",
                                ))
                            }
                        }
                    }
                }
            }
        }
        ConflictContentKind::Binary | ConflictContentKind::TooLarge => Err(ApiError::new(
            ErrorCode::UnsupportedOperation,
            "Built-in text editor cannot save binary or oversized conflict content",
        )),
    }
}

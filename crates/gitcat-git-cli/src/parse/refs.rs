//! `for-each-ref` records and upstream tracking counts.

use super::*;

#[derive(Debug, Clone)]
pub(crate) struct ParsedRef {
    pub oid: String,
    pub branch: Option<BranchInfo>,
    pub label: RefLabel,
    pub symbolic_target: Option<String>,
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

pub(super) fn parse_track(value: &[u8]) -> (Option<u32>, Option<u32>) {
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

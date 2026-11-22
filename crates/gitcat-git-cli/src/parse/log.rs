//! Commit log, search and commit detail records.

use super::*;

#[derive(Debug)]
pub(crate) struct ParsedCommitDetails {
    pub details: CommitDetails,
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

pub(super) fn parse_time(seconds: &[u8], formatted: &[u8]) -> ApiResult<CommitTime> {
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

pub(super) fn parse_offset(value: &str) -> Option<i16> {
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

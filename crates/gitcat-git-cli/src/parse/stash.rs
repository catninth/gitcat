//! The stash graph: which commits a stash shows and which stay hidden.

use super::*;

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

pub(super) fn stash_label(message: &str) -> String {
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

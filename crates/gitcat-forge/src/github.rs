//! Minimal GitHub REST client: author resolution, pull requests and checks.
//!
//! The commit list is the endpoint that matters for authors: it answers with one hundred
//! commits per request, and each entry pairs the raw commit email with the
//! account GitHub matched it to. That mapping is the part no local heuristic
//! can reproduce -- an author who commits under a private address still
//! resolves, because GitHub holds the address-to-account link itself.

use gitcat_contracts::{
    ApiError, ApiResult, CheckState, CheckSummary, ErrorCode, ForgeRepository, PullRequestInfo,
    PullRequestState,
};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

/// One request covers a whole history page and then some.
const COMMITS_PER_REQUEST: u16 = 100;
/// More open pull requests than a branch sidebar can meaningfully decorate.
const PULLS_PER_REQUEST: u16 = 100;
/// The largest page the repository list serves.
pub(crate) const REPOS_PER_REQUEST: u16 = 100;
const API_VERSION: &str = "2022-11-28";

pub struct GitHubClient {
    http: reqwest::Client,
    api_base: String,
    token: Option<String>,
}

/// A commit email and the account GitHub matched it to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitAuthor {
    pub email: String,
    /// `None` when the service matched no account to this address, which is a
    /// fact worth caching in its own right.
    pub avatar_url: Option<String>,
}

impl GitHubClient {
    /// `host` selects the endpoint: github.com talks to the public API, any
    /// other host is treated as a GitHub Enterprise install, which serves the
    /// same routes under `/api/v3`.
    pub fn new(http: reqwest::Client, host: &str, token: Option<String>) -> Self {
        let host = host.to_ascii_lowercase();
        let api_base = if host == "github.com" || host == "www.github.com" {
            "https://api.github.com".to_owned()
        } else {
            format!("https://{host}/api/v3")
        };
        Self {
            http,
            api_base,
            token,
        }
    }

    /// Lists commits reachable from `sha` (or the default branch), returning
    /// every author the response resolved to an account.
    pub async fn commit_authors(
        &self,
        owner: &str,
        repo: &str,
        sha: Option<&str>,
    ) -> ApiResult<Vec<CommitAuthor>> {
        let mut path = format!(
            "/repos/{}/{}/commits?per_page={COMMITS_PER_REQUEST}",
            encode(owner),
            encode(repo),
        );
        if let Some(sha) = sha {
            path.push_str(&format!("&sha={}", encode(sha)));
        }

        let commits: Vec<CommitListItem> = self.get_json(&path).await?;
        Ok(commits.into_iter().filter_map(commit_author).collect())
    }

    /// Lists the pull requests currently open against this repository.
    ///
    /// Only the open ones: a branch row answers "is there something in flight
    /// for this branch", and every closed pull request would be answering a
    /// different question out of the same rate limit.
    pub async fn pull_requests(&self, owner: &str, repo: &str) -> ApiResult<Vec<PullRequestInfo>> {
        let path = format!(
            "/repos/{}/{}/pulls?state=open&per_page={PULLS_PER_REQUEST}&sort=updated&direction=desc",
            encode(owner),
            encode(repo),
        );

        let pulls: Vec<PullListItem> = self.get_json(&path).await?;
        Ok(pulls
            .into_iter()
            .map(pull_request)
            .filter(|pull| !pull.head_ref.is_empty() && !pull.head_oid.is_empty())
            .collect())
    }

    /// Rolls up everything that reported a result for one commit.
    ///
    /// Both halves are real and a repository can carry them at once: an
    /// external service posts commit statuses, a workflow reports check runs.
    /// Check runs are the newer of the two, so an install that does not serve
    /// them counts as having none rather than as a failed request.
    pub async fn commit_checks(
        &self,
        owner: &str,
        repo: &str,
        oid: &str,
    ) -> ApiResult<CheckSummary> {
        let owner = encode(owner);
        let repo = encode(repo);
        let sha = encode(oid);
        let status_path = format!("/repos/{owner}/{repo}/commits/{sha}/status");
        let runs_path = format!("/repos/{owner}/{repo}/commits/{sha}/check-runs");
        let (statuses, runs) = tokio::join!(
            self.get_json::<CombinedStatus>(&status_path),
            self.get_json::<CheckRunList>(&runs_path),
        );

        let statuses = statuses?;
        let runs = runs.unwrap_or_default();
        Ok(roll_up(oid, &statuses.statuses, &runs.check_runs))
    }

    /// The owner and name a repository answers to now, as `(owner, name)`.
    ///
    /// A transferred or renamed repository keeps answering at its old path:
    /// the service redirects the request, and the client follows it. A clone
    /// made before the move therefore still works while its remote URL names
    /// an owner the repository no longer belongs to.
    pub async fn repository_location(
        &self,
        owner: &str,
        repo: &str,
    ) -> ApiResult<Option<(String, String)>> {
        let path = format!("/repos/{}/{}", encode(owner), encode(repo));
        let item: RepoListItem = self.get_json(&path).await?;
        Ok(location(item))
    }

    /// One page of the repositories the signed-in account can reach.
    ///
    /// `affiliation` is what makes this the list a person expects: their own
    /// repositories, the ones they collaborate on, and the ones an
    /// organisation shares with them -- rather than only what they own.
    pub async fn repositories(&self, page: u32) -> ApiResult<Vec<ForgeRepository>> {
        let path = format!(
            "/user/repos?per_page={REPOS_PER_REQUEST}&page={page}&sort=updated&affiliation=owner,collaborator,organization_member",
        );
        let repos: Vec<RepoListItem> = self.get_json(&path).await?;
        Ok(repos.into_iter().filter_map(repository).collect())
    }

    /// Creates a repository owned by the connected account.
    ///
    /// It is left empty on purpose: GitCat initialises the local repository,
    /// and a service that wrote a first commit of its own would leave the two
    /// sides with unrelated histories.
    pub async fn create_repository(
        &self,
        name: &str,
        description: Option<&str>,
        private: bool,
    ) -> ApiResult<ForgeRepository> {
        let body = NewRepoBody {
            name,
            description,
            private,
            auto_init: false,
        };
        let created: RepoListItem = self.post_json("/user/repos", &body).await?;
        repository(created).ok_or_else(|| {
            ApiError::new(
                ErrorCode::NetworkFailed,
                "the hosting service did not describe the repository it created",
            )
        })
    }

    async fn get_json<T: DeserializeOwned>(&self, path: &str) -> ApiResult<T> {
        let mut request = self
            .http
            .get(format!("{}{path}", self.api_base))
            .header("accept", "application/vnd.github+json")
            .header("x-github-api-version", API_VERSION);
        if let Some(token) = &self.token {
            request = request.header("authorization", format!("Bearer {token}"));
        }

        let response = request.send().await.map_err(network_error)?;
        let status = response.status();
        if !status.is_success() {
            return Err(status_error(status, rate_limited(&response)));
        }

        response.json().await.map_err(|error| {
            ApiError::new(
                ErrorCode::NetworkFailed,
                "the hosting service returned an unexpected response",
            )
            .with_details(error.to_string())
        })
    }

    /// A request that changes something. A refusal carries the service's own
    /// wording as details, because "name already exists" is the whole answer
    /// and the status code alone does not say it.
    async fn post_json<B: Serialize, T: DeserializeOwned>(
        &self,
        path: &str,
        body: &B,
    ) -> ApiResult<T> {
        let mut request = self
            .http
            .post(format!("{}{path}", self.api_base))
            .header("accept", "application/vnd.github+json")
            .header("x-github-api-version", API_VERSION)
            .json(body);
        if let Some(token) = &self.token {
            request = request.header("authorization", format!("Bearer {token}"));
        }

        let response = request.send().await.map_err(network_error)?;
        let status = response.status();
        if !status.is_success() {
            let limited = rate_limited(&response);
            let said = response.text().await.ok().and_then(|body| refusal(&body));
            let error = status_error(status, limited);
            return Err(match said {
                Some(details) => error.with_details(details),
                None => error,
            });
        }

        response.json().await.map_err(|error| {
            ApiError::new(
                ErrorCode::NetworkFailed,
                "the hosting service returned an unexpected response",
            )
            .with_details(error.to_string())
        })
    }
}

/// What the service said about a request it refused.
fn refusal(body: &str) -> Option<String> {
    let parsed: ErrorBody = serde_json::from_str(body).ok()?;
    parsed.message.filter(|message| !message.is_empty())
}

fn commit_author(item: CommitListItem) -> Option<CommitAuthor> {
    let email = item.commit.author?.email?;
    if email.is_empty() {
        return None;
    }
    Some(CommitAuthor {
        email: email.to_ascii_lowercase(),
        avatar_url: item
            .author
            .and_then(|account| account.avatar_url)
            .filter(|url| !url.is_empty()),
    })
}

/// The commit list nests the recorded author under `commit` and the matched
/// account at the top level; the account is absent when GitHub knows no
/// matching user.
#[derive(Deserialize)]
struct CommitListItem {
    commit: CommitMeta,
    author: Option<Account>,
}

#[derive(Deserialize)]
struct CommitMeta {
    author: Option<AuthorMeta>,
}

#[derive(Deserialize)]
struct AuthorMeta {
    email: Option<String>,
}

#[derive(Deserialize)]
struct Account {
    avatar_url: Option<String>,
}

fn pull_request(item: PullListItem) -> PullRequestInfo {
    let state = if item.draft.unwrap_or(false) {
        PullRequestState::Draft
    } else if item.merged_at.is_some() {
        PullRequestState::Merged
    } else if item.state.as_deref() == Some("closed") {
        PullRequestState::Closed
    } else {
        PullRequestState::Open
    };
    PullRequestInfo {
        number: item.number,
        title: item.title.unwrap_or_default(),
        state,
        author: item
            .user
            .and_then(|user| user.login)
            .filter(|login| !login.is_empty()),
        head_ref: item.head.reference.unwrap_or_default(),
        head_oid: item.head.sha.unwrap_or_default(),
        head_owner: item
            .head
            .repo
            .and_then(|repo| repo.owner)
            .and_then(|owner| owner.login)
            .filter(|login| !login.is_empty()),
        base_ref: item.base.reference.unwrap_or_default(),
        url: item.html_url.unwrap_or_default(),
        updated_at: item.updated_at,
    }
}

/// A repository with no clone URL or no owner is dropped: it could not be the
/// target of a clone, which is the only reason this list exists.
fn repository(item: RepoListItem) -> Option<ForgeRepository> {
    let owner = item.owner.and_then(|owner| owner.login)?;
    let clone_url = item.clone_url.filter(|url| !url.is_empty())?;
    let name = item.name?;
    Some(ForgeRepository {
        full_name: item.full_name.unwrap_or(format!("{owner}/{name}")),
        owner,
        name,
        private: item.private.unwrap_or(false),
        fork: item.fork.unwrap_or(false),
        description: item.description.filter(|text| !text.is_empty()),
        default_branch: item.default_branch.filter(|branch| !branch.is_empty()),
        clone_url,
        ssh_url: item.ssh_url.filter(|url| !url.is_empty()),
        updated_at: item.pushed_at.or(item.updated_at),
    })
}

fn location(item: RepoListItem) -> Option<(String, String)> {
    let owner = item.owner.and_then(|owner| owner.login)?;
    let name = item.name?;
    (!owner.is_empty() && !name.is_empty()).then_some((owner, name))
}

/// Collapses both report kinds into one badge.
///
/// A single failure outranks anything still running, because a run that has
/// already failed will not be un-failed by the rest finishing. `Neutral` is
/// reserved for the case where everything that reported was skipped: calling
/// that a success would claim a green tick nothing actually earned.
fn roll_up(oid: &str, statuses: &[CommitStatus], runs: &[CheckRun]) -> CheckSummary {
    let mut total = 0u32;
    let mut failed = 0u32;
    let mut pending = 0u32;
    let mut succeeded = 0u32;

    for status in statuses {
        total += 1;
        match status.state.as_deref() {
            Some("success") => succeeded += 1,
            Some("pending") | None => pending += 1,
            _ => failed += 1,
        }
    }

    for run in runs {
        total += 1;
        match run.conclusion.as_deref() {
            Some("success") => succeeded += 1,
            // Reported, deliberately inconclusive.
            Some("neutral") | Some("skipped") | Some("stale") => {}
            Some(_) => failed += 1,
            // Queued, waiting or still running: no conclusion yet.
            None => pending += 1,
        }
    }

    let state = if failed > 0 {
        CheckState::Failure
    } else if pending > 0 {
        CheckState::Pending
    } else if total == 0 {
        CheckState::None
    } else if succeeded > 0 {
        CheckState::Success
    } else {
        CheckState::Neutral
    };

    CheckSummary {
        oid: oid.to_owned(),
        state,
        total,
        failed,
        pending,
    }
}

/// `ref` is a keyword, so the head and base branch names are renamed rather
/// than spelled `r#ref`.
#[derive(Deserialize)]
struct PullListItem {
    number: u64,
    title: Option<String>,
    state: Option<String>,
    draft: Option<bool>,
    merged_at: Option<String>,
    updated_at: Option<String>,
    html_url: Option<String>,
    user: Option<UserMeta>,
    #[serde(default)]
    head: PullRef,
    #[serde(default)]
    base: PullRef,
}

#[derive(Default, Deserialize)]
struct PullRef {
    #[serde(rename = "ref")]
    reference: Option<String>,
    sha: Option<String>,
    repo: Option<RepoMeta>,
}

#[derive(Deserialize)]
struct RepoMeta {
    owner: Option<UserMeta>,
}

#[derive(Deserialize)]
struct UserMeta {
    login: Option<String>,
}

#[derive(Deserialize)]
struct RepoListItem {
    name: Option<String>,
    full_name: Option<String>,
    private: Option<bool>,
    fork: Option<bool>,
    description: Option<String>,
    default_branch: Option<String>,
    clone_url: Option<String>,
    ssh_url: Option<String>,
    updated_at: Option<String>,
    /// When the repository last received a commit, which sorts a list of
    /// working repositories more usefully than a settings change does.
    pushed_at: Option<String>,
    owner: Option<UserMeta>,
}

/// The body a repository is created with. `auto_init` stays off; see
/// `create_repository`.
#[derive(Serialize)]
struct NewRepoBody<'a> {
    name: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<&'a str>,
    private: bool,
    auto_init: bool,
}

#[derive(Deserialize)]
struct ErrorBody {
    message: Option<String>,
}

#[derive(Default, Deserialize)]
struct CombinedStatus {
    #[serde(default)]
    statuses: Vec<CommitStatus>,
}

#[derive(Deserialize)]
struct CommitStatus {
    state: Option<String>,
}

#[derive(Default, Deserialize)]
struct CheckRunList {
    #[serde(default)]
    check_runs: Vec<CheckRun>,
}

#[derive(Deserialize)]
struct CheckRun {
    conclusion: Option<String>,
}

/// GitHub answers an exhausted rate limit with 403 or 429 and no remaining
/// quota, which is worth telling apart from a rejected credential.
fn rate_limited(response: &reqwest::Response) -> bool {
    response
        .headers()
        .get("x-ratelimit-remaining")
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.trim() == "0")
}

fn status_error(status: reqwest::StatusCode, rate_limited: bool) -> ApiError {
    if rate_limited {
        return ApiError::new(
            ErrorCode::NetworkFailed,
            "the hosting service rate limit is exhausted; add a token to raise it",
        );
    }
    match status.as_u16() {
        401 | 403 => ApiError::new(
            ErrorCode::AuthenticationRequired,
            "the hosting service rejected the stored token",
        ),
        404 => ApiError::new(
            ErrorCode::InvalidRequest,
            "the hosting service does not expose this repository",
        ),
        422 => ApiError::new(
            ErrorCode::InvalidRequest,
            "the hosting service refused the request",
        ),
        _ => ApiError::new(
            ErrorCode::NetworkFailed,
            "the hosting service request failed",
        )
        .with_details(status.to_string()),
    }
}

fn network_error(error: reqwest::Error) -> ApiError {
    let code = if error.is_timeout() {
        ErrorCode::Timeout
    } else {
        ErrorCode::NetworkFailed
    };
    ApiError::new(code, "could not reach the hosting service").with_details(error.to_string())
}

/// Percent-encodes a single path or query segment. Owner, repository and
/// revision names are validated elsewhere, but they reach here as plain
/// strings and must not be able to open a new path.
fn encode(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(byte as char)
            }
            _ => encoded.push_str(&format!("%{byte:02X}")),
        }
    }
    encoded
}

#[cfg(test)]
mod tests {
    use super::*;

    fn client(host: &str) -> GitHubClient {
        GitHubClient::new(reqwest::Client::new(), host, None)
    }

    #[test]
    fn public_github_and_enterprise_use_different_api_roots() {
        assert_eq!(client("github.com").api_base, "https://api.github.com");
        assert_eq!(client("GitHub.com").api_base, "https://api.github.com");
        assert_eq!(
            client("git.example.test").api_base,
            "https://git.example.test/api/v3"
        );
    }

    #[test]
    fn path_segments_cannot_open_a_new_path() {
        assert_eq!(encode("ikoli"), "ikoli");
        assert_eq!(encode("../../users"), "..%2F..%2Fusers");
        assert_eq!(encode("a b"), "a%20b");
    }

    #[test]
    fn an_unmatched_author_is_kept_so_the_miss_can_be_cached() {
        let payload = r#"[
            {"commit":{"author":{"email":"A@Example.test"}},"author":{"avatar_url":"https://host.test/a.png"}},
            {"commit":{"author":{"email":"b@example.test"}},"author":null},
            {"commit":{"author":null},"author":{"avatar_url":"https://host.test/c.png"}}
        ]"#;
        let items: Vec<CommitListItem> = serde_json::from_str(payload).unwrap();
        let authors: Vec<CommitAuthor> = items.into_iter().filter_map(commit_author).collect();

        assert_eq!(
            authors,
            vec![
                CommitAuthor {
                    email: "a@example.test".into(),
                    avatar_url: Some("https://host.test/a.png".into()),
                },
                // Seen by the service, matched to nobody.
                CommitAuthor {
                    email: "b@example.test".into(),
                    avatar_url: None,
                },
            ]
        );
    }

    #[test]
    fn a_draft_is_its_own_state_and_a_fork_head_keeps_its_owner() {
        let payload = r#"[
            {"number":7,"title":"Add lanes","state":"open","draft":true,
             "html_url":"https://github.test/o/r/pull/7","user":{"login":"ikoli"},
             "head":{"ref":"feat/lanes","sha":"abc","repo":{"owner":{"login":"ikoli"}}},
             "base":{"ref":"main"}},
            {"number":8,"title":"From a fork","state":"open",
             "html_url":"https://github.test/o/r/pull/8","user":{"login":"other"},
             "head":{"ref":"main","sha":"def","repo":{"owner":{"login":"other"}}},
             "base":{"ref":"main"}},
            {"number":9,"state":"open","head":{"ref":"","sha":""},"base":{"ref":"main"}}
        ]"#;
        let items: Vec<PullListItem> = serde_json::from_str(payload).unwrap();
        let pulls: Vec<PullRequestInfo> = items
            .into_iter()
            .map(pull_request)
            .filter(|pull| !pull.head_ref.is_empty() && !pull.head_oid.is_empty())
            .collect();

        assert_eq!(pulls.len(), 2, "the head-less entry is dropped");
        assert_eq!(pulls[0].state, PullRequestState::Draft);
        assert_eq!(pulls[0].head_ref, "feat/lanes");
        assert_eq!(pulls[0].head_owner.as_deref(), Some("ikoli"));
        // Same branch name, different repository: the owner is what tells a
        // local `main` apart from a fork's.
        assert_eq!(pulls[1].state, PullRequestState::Open);
        assert_eq!(pulls[1].head_owner.as_deref(), Some("other"));
    }

    fn statuses(states: &[&str]) -> Vec<CommitStatus> {
        states
            .iter()
            .map(|state| CommitStatus {
                state: Some((*state).to_owned()),
            })
            .collect()
    }

    fn runs(conclusions: &[Option<&str>]) -> Vec<CheckRun> {
        conclusions
            .iter()
            .map(|conclusion| CheckRun {
                conclusion: conclusion.map(str::to_owned),
            })
            .collect()
    }

    #[test]
    fn nothing_reported_is_not_a_success() {
        let summary = roll_up("abc", &[], &[]);
        assert_eq!(summary.state, CheckState::None);
        assert_eq!(summary.total, 0);
    }

    #[test]
    fn a_failure_outranks_a_run_that_is_still_going() {
        let summary = roll_up(
            "abc",
            &statuses(&["success"]),
            &runs(&[Some("failure"), None]),
        );
        assert_eq!(summary.state, CheckState::Failure);
        assert_eq!(summary.total, 3);
        assert_eq!(summary.failed, 1);
        assert_eq!(summary.pending, 1);
    }

    #[test]
    fn a_pending_status_holds_the_badge_back_from_green() {
        let summary = roll_up("abc", &statuses(&["success", "pending"]), &[]);
        assert_eq!(summary.state, CheckState::Pending);
    }

    #[test]
    fn an_all_skipped_commit_is_neutral_rather_than_green() {
        let summary = roll_up("abc", &[], &runs(&[Some("skipped"), Some("neutral")]));
        assert_eq!(summary.state, CheckState::Neutral);
        assert_eq!(summary.total, 2);
        assert_eq!(summary.failed, 0);
    }

    #[test]
    fn both_report_kinds_count_towards_the_same_badge() {
        let summary = roll_up(
            "abc",
            &statuses(&["success"]),
            &runs(&[Some("success"), Some("skipped")]),
        );
        assert_eq!(summary.state, CheckState::Success);
        assert_eq!(summary.total, 3);
    }

    #[test]
    fn a_cancelled_run_counts_as_failed() {
        let summary = roll_up("abc", &[], &runs(&[Some("cancelled")]));
        assert_eq!(summary.state, CheckState::Failure);
    }

    /// A commit nothing reported on comes back as `"state":"pending"` with no
    /// statuses in it, so the top-level verdict is deliberately not read --
    /// the array is. Confirmed against the live API on 2026-08-27.
    #[test]
    fn an_empty_combined_status_is_not_pending_however_it_labels_itself() {
        let combined: CombinedStatus =
            serde_json::from_str(r#"{"state":"pending","total_count":0,"statuses":[]}"#).unwrap();
        let runs: CheckRunList =
            serde_json::from_str(r#"{"total_count":0,"check_runs":[]}"#).unwrap();

        let summary = roll_up("abc", &combined.statuses, &runs.check_runs);
        assert_eq!(summary.state, CheckState::None);
    }

    #[test]
    fn a_finished_check_run_is_read_from_its_conclusion() {
        let payload = r#"{"total_count":1,"check_runs":[
            {"name":"test-windows","status":"completed","conclusion":"success"}
        ]}"#;
        let runs: CheckRunList = serde_json::from_str(payload).unwrap();

        let summary = roll_up("abc", &[], &runs.check_runs);
        assert_eq!(summary.state, CheckState::Success);
        assert_eq!(summary.total, 1);
    }

    #[test]
    fn a_repository_without_a_clone_url_is_dropped() {
        let payload = r#"[
            {"name":"gitcat","full_name":"RisDN/gitcat","private":false,"fork":false,
             "clone_url":"https://github.com/RisDN/gitcat.git","ssh_url":"git@github.com:RisDN/gitcat.git",
             "default_branch":"main","pushed_at":"2026-08-27T10:00:00Z","owner":{"login":"RisDN"}},
            {"name":"broken","owner":{"login":"RisDN"}}
        ]"#;
        let items: Vec<RepoListItem> = serde_json::from_str(payload).unwrap();
        let repos: Vec<ForgeRepository> = items.into_iter().filter_map(repository).collect();

        assert_eq!(repos.len(), 1);
        assert_eq!(repos[0].full_name, "RisDN/gitcat");
        assert_eq!(repos[0].owner, "RisDN");
        // The last commit sorts a working list better than a settings change.
        assert_eq!(repos[0].updated_at.as_deref(), Some("2026-08-27T10:00:00Z"));
    }

    #[test]
    fn a_moved_repository_reports_where_it_lives_now() {
        // What `/repos/RisDN/gitcat` answers after the redirect is followed.
        let item: RepoListItem = serde_json::from_str(
            r#"{"name":"gitcat","full_name":"catninth/gitcat","owner":{"login":"catninth"}}"#,
        )
        .unwrap();
        assert_eq!(
            location(item),
            Some(("catninth".to_owned(), "gitcat".to_owned()))
        );

        let ownerless: RepoListItem = serde_json::from_str(r#"{"name":"gitcat"}"#).unwrap();
        assert_eq!(location(ownerless), None);
    }

    #[test]
    fn a_new_repository_is_asked_for_empty() {
        let body = NewRepoBody {
            name: "gitcat",
            description: None,
            private: true,
            auto_init: false,
        };
        let json = serde_json::to_value(&body).expect("serialises");

        assert_eq!(json["name"], "gitcat");
        assert_eq!(json["private"], true);
        // An initialised remote would leave the two sides unrelated.
        assert_eq!(json["auto_init"], false);
        assert!(
            json.get("description").is_none(),
            "a description that was not given is left out rather than sent empty"
        );
    }

    #[test]
    fn a_refusal_keeps_the_wording_the_service_used() {
        assert_eq!(
            refusal(r#"{"message":"Repository creation failed."}"#).as_deref(),
            Some("Repository creation failed."),
        );
        assert_eq!(refusal(r#"{"message":""}"#), None);
        assert_eq!(refusal("<html>gateway timeout</html>"), None);
    }
}

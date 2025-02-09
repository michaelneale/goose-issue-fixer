use anyhow::{anyhow, Result};
use octocrab::Octocrab;
use serde::Deserialize;
use std::env;

pub struct GitHubClient {
    client: Octocrab,
    owner: String,
    repo: String,
}

#[derive(Debug)]
pub struct PullRequestDetails {
    pub number: u64,
    pub title: String,
    pub description: String,
    pub comments: Vec<String>,
    pub state: String,
    pub mergeable: Option<bool>,
    pub merged: bool,
    pub workflows: Vec<WorkflowRun>,
    pub commits: Vec<CommitInfo>,
    pub diff: String,
}

#[derive(Debug)]
pub struct WorkflowRun {
    pub id: String,
    pub name: String,
    pub status: String,
    pub conclusion: Option<String>,
    pub html_url: String,
    pub logs_url: Option<String>,
    pub head_sha: String,
}

#[derive(Debug)]
pub struct CommitInfo {
    pub sha: String,
    pub message: String,
    pub author: String,
    pub author_email: Option<String>,
    pub url: String,
}

#[derive(Debug)]
pub struct PullRequestSummary {
    pub number: u64,
    pub title: String,
    pub state: String,
    pub merged: bool,
}

impl GitHubClient {
    pub fn new(owner: String, repo: String) -> Result<Self> {
        let token = env::var("GITHUB_TOKEN")
            .map_err(|_| anyhow!("GITHUB_TOKEN environment variable not set"))?;

        let client = Octocrab::builder().personal_token(token).build()?;

        Ok(Self {
            client,
            owner,
            repo,
        })
    }

    pub async fn list_recent_merged_prs(&self, limit: usize) -> Result<Vec<PullRequestSummary>> {
        let mut all_prs = Vec::new();
        let mut page = 1u32;

        while all_prs.len() < limit {
            let prs = self
                .client
                .pulls(&self.owner, &self.repo)
                .list()
                .state(octocrab::params::State::Closed) // We want closed PRs
                .sort(octocrab::params::pulls::Sort::Created)
                .direction(octocrab::params::Direction::Descending)
                .per_page(100)
                .page(page)
                .send()
                .await?;

            let items = prs.items;
            if items.is_empty() {
                break;
            }

            for pr in items {
                if all_prs.len() >= limit {
                    break;
                }

                // Only include merged PRs (those with a merged_at timestamp)
                if pr.merged_at.is_some() {
                    let state = match pr.state {
                        Some(state) => match state {
                            octocrab::models::IssueState::Open => "open",
                            octocrab::models::IssueState::Closed => "closed",
                            _ => "other",
                        },
                        None => "unknown",
                    }
                    .to_string();

                    all_prs.push(PullRequestSummary {
                        number: pr.number,
                        title: pr.title.unwrap_or_default(),
                        state,
                        merged: true,
                    });
                }
            }

            page += 1;
        }

        Ok(all_prs)
    }

    pub async fn get_pull_request_details(&self, pr_number: u64) -> Result<PullRequestDetails> {
        // Get PR details
        let pr = self
            .client
            .pulls(&self.owner, &self.repo)
            .get(pr_number)
            .await?;

        // Get PR comments
        let comments = self
            .client
            .issues(&self.owner, &self.repo)
            .list_comments(pr_number)
            .send()
            .await?;

        let comments = comments
            .items
            .into_iter()
            .map(|comment| comment.body.unwrap_or_default())
            .collect();

        // Convert state to string
        let state = match pr.state {
            Some(state) => match state {
                octocrab::models::IssueState::Open => "open",
                octocrab::models::IssueState::Closed => "closed",
                _ => "other",
            },
            None => "unknown",
        }
        .to_string();

        // Check if PR was merged
        let merged = pr.merged_at.is_some();

        // Get workflow runs for the PR's commit
        let head_sha = pr.head.sha;
        let workflows = self.get_workflow_runs(&head_sha).await?;

        // Get commits in the PR
        let commits = self.get_pr_commits(pr_number).await?;

        // Get PR diff
        let diff = self.get_pr_diff(pr_number).await?;

        Ok(PullRequestDetails {
            number: pr.number,
            title: pr.title.unwrap_or_default(),
            description: pr.body.unwrap_or_default(),
            comments,
            state,
            mergeable: pr.mergeable,
            merged,
            workflows,
            commits,
            diff,
        })
    }

    async fn get_workflow_runs(&self, commit_sha: &str) -> Result<Vec<WorkflowRun>> {
        #[derive(Deserialize)]
        struct Response {
            total_count: usize,
            workflow_runs: Vec<Run>,
        }

        #[derive(Deserialize)]
        struct Run {
            id: u64,
            name: Option<String>,
            head_sha: String,
            status: Option<String>,
            conclusion: Option<String>,
            html_url: String,
            run_attempt: Option<u64>,
        }

        #[derive(serde::Serialize)]
        struct Params<'a> {
            head_sha: &'a str,
        }

        let params = Params {
            head_sha: commit_sha,
        };
        let response: Response = self
            .client
            .get(
                format!("/repos/{}/{}/actions/runs", self.owner, self.repo),
                Some(&params),
            )
            .await?;

        let mut runs = Vec::new();
        for run in response.workflow_runs {
            // Get logs URL if available
            let logs_url = run.run_attempt.map(|attempt| {
                format!(
                    "https://api.github.com/repos/{}/{}/actions/runs/{}/attempts/{}/logs",
                    self.owner, self.repo, run.id, attempt
                )
            });

            runs.push(WorkflowRun {
                id: run.id.to_string(),
                name: run.name.unwrap_or_else(|| "Unknown".to_string()),
                status: run.status.unwrap_or_else(|| "unknown".to_string()),
                conclusion: run.conclusion,
                html_url: run.html_url,
                logs_url,
                head_sha: run.head_sha,
            });
        }

        Ok(runs)
    }

    async fn get_pr_commits(&self, pr_number: u64) -> Result<Vec<CommitInfo>> {
        #[derive(Deserialize)]
        struct CommitResponse {
            sha: String,
            commit: Commit,
            html_url: String,
        }

        #[derive(Deserialize)]
        struct Commit {
            message: String,
            author: CommitAuthor,
        }

        #[derive(Deserialize)]
        struct CommitAuthor {
            name: String,
            email: Option<String>,
        }

        let route = format!(
            "/repos/{}/{}/pulls/{}/commits",
            self.owner, self.repo, pr_number
        );
        let commits: Vec<CommitResponse> = self.client.get(&route, None::<&()>).await?;

        let mut commit_infos = Vec::new();
        for commit in commits {
            commit_infos.push(CommitInfo {
                sha: commit.sha,
                message: commit.commit.message,
                author: commit.commit.author.name,
                author_email: commit.commit.author.email,
                url: commit.html_url,
            });
        }

        Ok(commit_infos)
    }

    async fn get_pr_diff(&self, pr_number: u64) -> Result<String> {
        // Use reqwest directly to get the diff since octocrab doesn't support custom Accept headers
        let url = format!(
            "https://api.github.com/repos/{}/{}/pulls/{}",
            self.owner, self.repo, pr_number
        );

        let token = env::var("GITHUB_TOKEN")?;
        let client = reqwest::Client::new();
        let response = client
            .get(&url)
            .header("Accept", "application/vnd.github.v3.diff")
            .header("Authorization", format!("token {}", token))
            .header("User-Agent", "Goose-PR-Analyzer/1.0")
            .send()
            .await?
            .text()
            .await?;

        Ok(response)
    }
}

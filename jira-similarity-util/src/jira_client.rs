use anyhow::{anyhow, Result};
use reqwest::{Client, header::{HeaderMap, HeaderValue, AUTHORIZATION, ACCEPT}};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use serde::{Deserialize, Serialize};
use serde::de::DeserializeOwned;
use std::time::Duration;
use futures::future::join_all;

pub struct JiraClient {
    client: Client,
    base_url: String,
}

impl JiraClient {
    /// Create a new JiraClient with basic auth
    /// 
    /// # Arguments
    /// * `base_url` - Base URL for Jira instance (e.g., "https://your-domain.atlassian.net")
    /// * `email` - Jira account email
    /// * `api_token` - Jira API token
    pub fn new(email: String, api_token: String, base_url: String) -> Result<Self> {
        // Create basic auth header
        let auth = format!("{}:{}", email, api_token);
        let encoded_auth = BASE64.encode(auth.as_bytes());
        let auth_header = format!("Basic {}", encoded_auth);

        // Setup headers
        let mut headers = HeaderMap::new();
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&auth_header).map_err(|e| anyhow!("Invalid header value: {}", e))?,
        );

        headers.insert(
            ACCEPT,
            HeaderValue::from_str("application/json")?,
        );

        // Configure client with reasonable defaults
        let client = Client::builder()
            .default_headers(headers)
            .timeout(Duration::from_secs(30))
            .connection_verbose(true)
            .build()?;

        Ok(Self {
            client,
            base_url: base_url.trim_end_matches('/').to_string(),
        })
    }

    /// Make a GET request to a Jira endpoint
    async fn get<T: DeserializeOwned>(&self, endpoint: &str) -> Result<T> {
        let url = format!("{}{}", self.base_url, endpoint);
        
        let response = self.client
            .get(&url)
            .send()
            .await?;

        // Check if request was successful
        if !response.status().is_success() {
            return Err(anyhow!(
                "Jira API request failed: {} - {}",
                response.status(),
                response.text().await?
            ));
        }

        // Parse JSON response
        let data = response.json::<T>().await?;
        Ok(data)
    }

    /// Make a POST request to a Jira endpoint
    async fn post<T: DeserializeOwned, B: serde::Serialize>(
        &self,
        endpoint: &str,
        body: &B,
    ) -> Result<T> {
        let url = format!("{}{}", self.base_url, endpoint);
        
        let response = self.client
            .post(&url)
            .json(body)
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(anyhow!(
                "Jira API request failed: {} - {}",
                response.status(),
                response.text().await?
            ));
        }

        let data = response.json::<T>().await?;
        Ok(data)
    }

    // API Methods

    pub async fn get_issue(&self, issue_key: String) -> Result<Issue> {
        let mut issue: Issue = self.get(&format!("/rest/agile/1.0/issue/{}", issue_key)).await?;
        
        // Fetch pull requests and handle potential errors
        match self.get_pull_requests(issue.id.clone()).await {
            Ok(pull_requests) => {
                issue.pull_requests = pull_requests;
                Ok(issue)
            },
            Err(e) => {
                println!("Failed to fetch pull requests: {}", e);
                Ok(issue)
            }
        }
    }

    pub async fn get_pull_requests(&self, issue_id: String) -> Result<Vec<PullRequest>> {
        // TODO: Could make this generic for other services (bit bucket etc)
        self.get(&format!("/rest/dev-status/latest/issue/detail?issueId={}&applicationType=GitHub&dataType=pullrequest", issue_id))
            .await
            .map(|response: GetPullRequestsResponse| {
                response.details
                    .into_iter()
                    .flat_map(|detail| detail.pull_requests)
                    .collect()
            })
    }

    /// Search for issues using JQL
    /// 
    /// # Arguments
    /// * `jql` - JQL query string
    /// * `start_at` - The starting index of the returned issues. Base index: 0
    /// * `max_results` - The maximum number of issues to return (default: 50)
    /// * `fields` - The list of fields to return for each issue
    /// * `validate_query` - Whether to validate the JQL query
    /// * `expand` - The parameters to expand
    /// 
    /// # Example
    /// ```rust
    /// let request = SearchRequest {
    ///     jql: "project = MYPROJECT AND status = Open".to_string(),
    ///     start_at: Some(0),
    ///     max_results: Some(50),
    ///     fields: Some(vec!["summary".to_string(), "status".to_string()]),
    ///     validate_query: Some(true),
    ///     expand: None,
    /// };
    /// let results = client.search(request).await?;
    /// ```
    pub async fn search(&self, request: SearchRequest) -> Result<SearchResponse> {
        self.post("/rest/api/2/search", &request).await
    }

    /// Convenience method for simple JQL searches
    pub async fn search_issues(&self, jql: &str) -> Result<Vec<Issue>> {
        let request = SearchRequest {
            jql: jql.to_string(),
            start_at: 0,
            max_results: 15,
            fields: vec!["*all".to_string()],
            validate_query: true,
            expand: vec!["".to_string()],
        };

        let response = self.search(request).await?;
        Ok(response.issues)
    }

    /// Search with pagination
    pub async fn search_with_pagination(
        &self,
        jql: &str,
        start_at: i32,
        max_results: i32,
    ) -> Result<SearchResponse> {
        let request = SearchRequest {
            jql: jql.to_string(),
            start_at: start_at,
            max_results: max_results,
            fields: vec!["*all".to_string()],
            validate_query: true,
            expand: vec!["".to_string()],
        };

        self.search(request).await
    }

    /// Build a JQL query to find similar issues based on various relationships
    pub fn build_similarity_query(&self, issue: &Issue) -> Vec<(String, Vec<String>)> {

        let mut issue_keys_to_include = Vec::new();
        let mut issue_keys_to_exclude = Vec::new();

        let mut cross_project_conditions = Vec::new();
        let mut project_scoped_conditions = Vec::new();
        let fields_to_return = vec![
            "summary",
            "status",
            "issuetype",
            "components",
            "labels",
            "parent",
            "epic",
            "issuelinks",
            "project",
            "description",
            "comment",
            "creator",
        ].iter().map(|&s| s.to_string()).collect::<Vec<_>>();

        issue_keys_to_exclude.push(issue.key.clone());

        // Linked Issues
        if let Some(links) = &issue.fields.issue_links {
            if !links.is_empty() {
                let linked_keys: Vec<String> = links.iter()
                    .filter_map(|link| link.inward_issue.as_ref().map(|i| i.key.clone()))
                    .collect();
                
                issue_keys_to_include.append(&mut linked_keys.clone());
            }
        }

        // Issues in the same epic
        if let Some(epic) = &issue.fields.epic {
            cross_project_conditions.push(format!("\"Epic Link\" = {}", epic.id));
        }

        // Parent relationship conditions
        if let Some(parent) = &issue.fields.parent {
            // Include both:
            // - The parent ticket itself
            // - Sibling issues (issues with the same parent)
            issue_keys_to_include.push(parent.key.clone());
            cross_project_conditions.push(format!("parent = {}", parent.key));
        }

        // Issues with the same components
        if !issue.fields.components.is_empty() {
            let component_ids: Vec<String> = issue.fields.components.iter()
                .map(|c| c.id.clone())
                .collect();
            project_scoped_conditions.push(format!("component in ({})", component_ids.join(",")));
        }

        // Issues with the same labels
        if !issue.fields.labels.is_empty() {
            let label_jql = format!("labels in ({})", issue.fields.labels.join(","));
            project_scoped_conditions.push(label_jql);
        }

        let exclude_issues_jql = format!("key NOT in ({})", issue_keys_to_exclude.into_iter().map(|key| format!("\"{}\"", key)).collect::<Vec<_>>().join(","));


        let mut directly_linked_issues_jql = "".to_owned();
        if !issue_keys_to_include.is_empty() {
            let include_issues_jql = format!("key in ({})", issue_keys_to_include.into_iter().map(|key| format!("\"{}\"", key)).collect::<Vec<_>>().join(","));
            directly_linked_issues_jql.push_str(&include_issues_jql);
        }
       if !cross_project_conditions.is_empty() {
            if !directly_linked_issues_jql.is_empty() { 
                directly_linked_issues_jql.push_str(" OR ");
            }
            directly_linked_issues_jql.push_str(
                &format!("{}", cross_project_conditions.join(" OR "))
            );
        }
        if !directly_linked_issues_jql.is_empty() { 
            directly_linked_issues_jql = format!("({directly_linked_issues_jql}) AND {exclude_issues_jql} ORDER BY updated DESC");
        }

       let mut similar_issues_jql = "".to_owned();
       if !project_scoped_conditions.is_empty() {
            similar_issues_jql.push_str(
                &format!("{}", project_scoped_conditions.join(" OR "))
            );
        }
        if !similar_issues_jql.is_empty() { 
            similar_issues_jql = format!("({similar_issues_jql}) AND {exclude_issues_jql} AND project = \"{}\" ORDER BY updated DESC",
                issue.fields.project.key,
            );
        }

        let mut queries:Vec<(String, Vec<String>)> = [].to_vec();

        if !directly_linked_issues_jql.is_empty() {
            queries.push((directly_linked_issues_jql, fields_to_return.clone()));
        }

        if !similar_issues_jql.is_empty() {
            queries.push((similar_issues_jql, fields_to_return.clone()));
        }

        return queries
    }
    
    /// Find similar issues based on various criteria
    pub async fn find_similar_issues(&self, issue: &Issue) -> Result<Vec<Issue>> {
        // Build the similarity query
        let queries = self.build_similarity_query(&issue);
        
        let requests: Vec<SearchRequest> = queries
            .into_iter()
            .map(|(jql, fields)| {
                // Create the search request
                return SearchRequest {
                    jql,
                    start_at: 0,
                    max_results: 50,
                    fields: fields,
                    validate_query: true,
                    expand: vec!["".to_string()],
                }
            })
            .collect();

        let futures = requests
            .into_iter()
            .map(|request| async {
                match self.search(request).await {
                    Ok(response) => response.issues,
                    Err(e) => {
                        println!("Failed to search for issue: {}", e);
                        Vec::new()
                    }
                }
            });
        
        let issues: Vec<Issue> = join_all(futures).await.into_iter().flatten().collect();

        Ok(issues)
    }
}

// API Wrapper Types

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct GetPullRequestsResponse {
    #[serde(rename = "detail")]
    pub details: Vec<DevelopmentDetails>,
}

#[derive(Debug, serde::Deserialize, Serialize)]
pub struct DevelopmentDetails {
    #[serde(rename = "pullRequests")]
    pub pull_requests: Vec<PullRequest>,
}

// Request type for search
#[derive(Debug, Deserialize, Serialize)]
pub struct SearchRequest {
    pub jql: String,
    #[serde(rename = "startAt")]
    pub start_at: i32,
    #[serde(rename = "maxResults")]
    pub max_results: i32,
    pub fields: Vec<String>,
    #[serde(rename = "validateQuery")]
    pub validate_query: bool,
    pub expand: Vec<String>,
}

// Response type for search
#[derive(Serialize, Deserialize, Debug)]
pub struct SearchResponse {
    pub expand: String,
    #[serde(rename = "startAt")]
    pub start_at: i32,
    #[serde(rename = "maxResults")]
    pub max_results: i32,
    pub total: i32,
    pub issues: Vec<Issue>,
}

#[derive(Debug, Serialize, serde::Deserialize, Clone)]
pub struct Issue {
    pub id: String,
    pub key: String,
    pub fields: Fields,
    #[serde(skip)]
    pub pull_requests: Vec<PullRequest>,
}

#[derive(Debug, Serialize, serde::Deserialize, Clone)]
pub struct Fields {
    pub labels: Vec<String>,
    pub description: Option<String>,
    pub status: Status,
    pub project: Project,
    pub summary: Option<String>,
    pub assignee: Option<Account>,
    pub comment: CommentWrapper,
    pub components: Vec<IdNameWrapper>,
    pub resolution: Option<IdNameWrapper>,
    #[serde(rename = "issuetype")]
    pub issue_type: IdNameWrapper,
    pub epic: Option<NumericIdNameWrapper>,
    pub parent: Option<IdKeyWrapper>,
    pub creator: Account,
    #[serde(rename="issuelinks")]
    pub issue_links: Option<Vec<IssueLink>>,
}

#[derive(Debug, Serialize, serde::Deserialize, Clone)]
pub struct Comment {
    pub body: String,
    pub id: String,
    pub author: Account,
}

#[derive(Debug, Serialize, serde::Deserialize, Clone)]
pub struct CommentWrapper {
    pub comments: Vec<Comment>,
}

#[derive(Debug, Serialize, serde::Deserialize, Clone)]
pub struct Account {
    #[serde(rename = "accountId")]
    pub id: String,
}

#[derive(Debug, Serialize, serde::Deserialize, Clone)]
pub struct Project {
    pub id: String,
    pub key: String,
}

#[derive(Debug, Serialize, serde::Deserialize, Clone)]
pub struct Status {
    pub name: Option<String>,
    #[serde(rename = "statusCategory")]
    pub category: NumericIdNameWrapper,
}

#[derive(Debug, Serialize, serde::Deserialize, Clone)]
pub struct IdNameWrapper {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Serialize, serde::Deserialize, Clone)]
pub struct NumericIdNameWrapper {
    pub name: String,
    pub id: u64,
}

#[derive(Debug, Serialize, serde::Deserialize, Clone)]
pub struct IdKeyWrapper {
    pub key: String,
    pub id: String,
}

#[derive(Debug, Serialize, serde::Deserialize, Clone)]
pub struct PullRequest {
    pub name: String,
    pub status: String,
    pub url: String,
    #[serde(rename = "repositoryName")]
    pub repository_name: String,
}

#[derive(Debug, Serialize, serde::Deserialize, Clone)]
pub struct IssueLink {
    pub id: String,
    #[serde(rename = "type")]
    pub link_type: IdNameWrapper,
    #[serde(rename = "inwardIssue")]
    pub inward_issue: Option<IdKeyWrapper>,
}
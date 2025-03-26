use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use dotenv::dotenv;
use std::env;
use jira_similarity_util::jira_client::JiraClient;
use jira_similarity_util::jira_client::Issue;
use jira_similarity_util::issue_contextualizer::IssueContextualizer;
use jira_similarity_util::jira_client::SearchRequest;
use git2::{Repository, Status};
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "src/templates/"]
struct Templates;

const INSTRUCTIONS_TEMPLATE: &str = include_str!("../src/templates/instructions-starting.txt");

/// Check if repository exists, is clean, and update it to latest main branch
fn prepare_git_repository(repo_path: &PathBuf) -> Result<()> {
    // Try to open the repository to verify it exists
    let repo = Repository::open(repo_path)
        .map_err(|_| anyhow::anyhow!("No git repository found at {}", repo_path.display()))?;

    // Check for uncommitted changes
    let statuses = repo.statuses(None)?;
    let has_changes = statuses.iter().any(|status| {
        let status = status.status();
        status.intersects(Status::INDEX_NEW | Status::INDEX_MODIFIED | Status::INDEX_DELETED |
                         Status::WT_NEW | Status::WT_MODIFIED | Status::WT_DELETED)
    });

    if has_changes {
        return Err(anyhow::anyhow!("Repository has uncommitted changes. Please commit or stash them before proceeding."));
    }

    // Find the default branch (main/master)
    let default_branch_names = ["main", "master"];
    let mut found_branch = None;

    for branch_name in default_branch_names {
        if let Ok(_branch) = repo.find_branch(branch_name, git2::BranchType::Local) {
            found_branch = Some(branch_name.to_string());
            break;
        }
    }

    let default_branch = found_branch.ok_or_else(|| 
        anyhow::anyhow!("Could not find main or master branch")
    )?;

    // Use system git command to fetch and checkout
    println!("Fetching latest changes...");
    let output = std::process::Command::new("git")
        .current_dir(repo_path)
        .arg("fetch")
        .arg("origin")
        .output()?;

    if !output.status.success() {
        return Err(anyhow::anyhow!(
            "Failed to fetch: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    println!("Checking out {}...", default_branch);
    let output = std::process::Command::new("git")
        .current_dir(repo_path)
        .args(["checkout", &default_branch])
        .output()?;

    if !output.status.success() {
        return Err(anyhow::anyhow!(
            "Failed to checkout {}: {}",
            default_branch,
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    let output = std::process::Command::new("git")
        .current_dir(repo_path)
        .args(["pull", "origin", &default_branch])
        .output()?;

    if !output.status.success() {
        return Err(anyhow::anyhow!(
            "Failed to pull latest changes: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    println!("Repository is ready");
    Ok(())
}

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Process a sprint ticket for similarity analysis
    Sprint {
        /// GitHub repository name
        #[arg(short, long)]
        repo: String,

        /// Cache directory for PR data
        #[arg(short, long, default_value = "/tmp/.jira_cache")]
        cache_dir: PathBuf,

        /// Directory where the git repository is checked out
        /// If not provided, will try to read from DEVELOPMENT_DIR environment variable
        #[arg(short, long)]
        development_dir: Option<PathBuf>,

        #[arg(short, long)]
        ticket: String,

        /// How many similar PR URLs to fetch
        #[arg(short = 'n', long, default_value = "10")]
        pr_load_count: usize,

        /// Minimum similarity score threshold (0.0 to 5.0)
        #[arg(short = 's', long, default_value = "1.0")]
        minimum_similarity_score: f32,

        /// Optional path to a file containing additional instructions
        #[arg(short = 'a', long)]
        additional_instructions: Option<PathBuf>,
    },

    /// Search for Jira issues using JQL
    Search {
        /// Keywords to search for in issue summary and description
        #[arg(short, long)]
        keywords: Option<String>,

        /// Components to filter by (comma-separated)
        #[arg(short, long)]
        components: Option<String>,

        /// Labels to filter by (comma-separated)
        #[arg(short, long)]
        labels: Option<String>,

        /// Projects to search in (comma-separated)
        #[arg(short, long)]
        projects: Option<String>,

        /// Output format: 'console' or 'file' (default: file)
        #[arg(short, long, default_value = "file")]
        output: String,

        /// Cache directory for output files
        #[arg(short, long, default_value = "/tmp/.jira_cache")]
        cache_dir: PathBuf,

        /// Maximum number of results to return
        #[arg(short = 'n', long, default_value = "50")]
        max_results: i32,
    }
}

async fn process_sprint_ticket(
    client: &JiraClient,
    repo: String,
    cache_dir: PathBuf,
    development_dir: PathBuf,
    ticket: String,
    pr_load_count: usize,
    minimum_similarity_score: f32,
    additional_instructions: Option<PathBuf>,
) -> Result<()> {
    // Create cache directory if it doesn't exist
    if !cache_dir.exists() {
        std::fs::create_dir_all(&cache_dir)?;
    }

    println!("Fetching issue {}...\n", ticket);
    let issue_to_fix = client.get_issue(ticket).await?;
    println!("Fetching issues that might be similar/related...\n");
    let similar_issues = client.find_similar_issues(&issue_to_fix).await?;

    println!("Calculating relevace scores for similar/related issues...\n");
    // Create a vector of indices and scores
    let mut scored_indices: Vec<(usize, f32)> = similar_issues.iter()
        .enumerate()
        .map(|(idx, _)| (idx, 0.0))  // We'll calculate scores in a moment
        .collect();

    // Calculate scores for each issue
    for (idx, score) in scored_indices.iter_mut() {
        let refs: Vec<&Issue> = vec![&similar_issues[*idx]];
        let ranked = IssueContextualizer::sort_issues_by_similarity(&issue_to_fix, refs);
        if let Some((_, s)) = ranked.first() {
            *score = *s;
        }
    }

    // Sort by score and filter by minimum threshold
    println!("Dropping issues that don't meet the minimum similarity threshold...\n");
    scored_indices.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    scored_indices.retain(|(_, score)| *score >= minimum_similarity_score);

    // Process issues in order
    println!("Fetching additional data for similar issues...\n");
    let mut processed_issues = Vec::new();
    for (idx, score) in scored_indices.into_iter().take(pr_load_count) {
        let mut issue = similar_issues[idx].clone();
        let pull_requests = client.get_pull_requests(issue.id.clone()).await?;
        issue.pull_requests = pull_requests;
        processed_issues.push((issue, score));
    }

    println!("Generating LLM Context...\n");
    let llm_context = IssueContextualizer::get_llm_context(&issue_to_fix, processed_issues);

    // Create issue-specific directory
    let issue_dir = cache_dir.join(&issue_to_fix.key);
    std::fs::create_dir_all(&issue_dir)?;

    // Write context to file
    let context_file = issue_dir.join("llm_context_issue_summary.md");
    std::fs::write(&context_file, &llm_context)?;
    println!("LLM context written to: {}", context_file.display());

    // Write raw JSON to files
    let mut joined_issues = Vec::new();
    joined_issues.push(issue_to_fix.clone());
    joined_issues.extend(similar_issues);
    let json_file = issue_dir.join("raw_issues.json");

    std::fs::write(&json_file, serde_json::to_string_pretty(&joined_issues).unwrap())?;
    println!("Raw Issue JSON written to: {}", json_file.display());

    // Load and process the instruction template
    println!("Generating instruction file...");
    let mut template_content = INSTRUCTIONS_TEMPLATE.to_string();
    
    // If additional instructions file is provided and exists, append its contents
    if let Some(additional_file) = additional_instructions {
        if additional_file.exists() {
            println!("Adding additional instructions from: {}", additional_file.display());
            let additional_content = std::fs::read_to_string(additional_file)?;
            template_content.push_str("\n\n# Additional Instructions\n");
            template_content.push_str("The following are additional instructions specific to this ticket. ");
            template_content.push_str("If any of these conflict with the above instructions, follow these additional instructions instead:\n\n");
            template_content.push_str(&additional_content);
        } else {
            println!("Warning: Additional instructions file not found at: {}", additional_file.display());
        }
    }
    
    // Get current user's username from home directory
    let username = env::var("USER").unwrap_or_else(|_| {
        dirs::home_dir()
            .and_then(|path| path.file_name().and_then(|name| name.to_str().map(|s| s.to_string())))
            .unwrap_or_else(|| "unknown".to_string())
    });
    
    // Create suggested branch name with format: <user>/goose/<ticket_key>
    let suggested_branch_name = format!("{}/goose/{}", username, issue_to_fix.key.to_lowercase());
    
    // Replace variables in template
    let instructions = template_content
        .replace("{repo}", &repo)
        .replace("{original_ticket_key}", &issue_to_fix.key)
        .replace("{issue_cache_directory}", &issue_dir.to_string_lossy())
        .replace("{development_dir}", &development_dir.to_string_lossy())
        .replace("{suggested_branch_name}", &suggested_branch_name);
    
    // Write processed instructions to the cache directory
    let instructions_file = issue_dir.join("instructions.txt");
    std::fs::write(&instructions_file, instructions)?;
    println!("Instructions written to: {}", instructions_file.display());

    // Run goose with the instructions file
    println!("\nRunning goose...");
    let output = std::process::Command::new("goose")
        .arg("run")
        .arg("-i")
        .arg(&instructions_file)
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit())
        .output()?;

    if !output.status.success() {
        return Err(anyhow::anyhow!("Goose execution failed"));
    }

    Ok(())
}

async fn process_search(
    client: &JiraClient,
    keywords: Option<String>,
    components: Option<String>,
    labels: Option<String>,
    projects: Option<String>,
    output: String,
    cache_dir: PathBuf,
    max_results: i32,
) -> Result<()> {
    // Build JQL query
    let mut conditions = Vec::new();

    // Add keyword search if provided
    if let Some(keywords) = keywords {
        conditions.push(format!("(summary ~ \"{}\" OR description ~ \"{}\")", keywords, keywords));
    }

    // Add component filter if provided
    if let Some(components) = components {
        let component_list: Vec<String> = components.split(',')
            .map(|s| format!("\"{}\"", s.trim()))
            .collect();
        conditions.push(format!("component in ({})", component_list.join(",")));
    }

    // Add label filter if provided
    if let Some(labels) = labels {
        let label_list: Vec<String> = labels.split(',')
            .map(|s| format!("\"{}\"", s.trim()))
            .collect();
        conditions.push(format!("labels in ({})", label_list.join(",")));
    }

    // Add project filter if provided
    if let Some(projects) = projects {
        let project_list: Vec<String> = projects.split(',')
            .map(|s| format!("\"{}\"", s.trim()))
            .collect();
        conditions.push(format!("project in ({})", project_list.join(",")));
    }

    // Combine all conditions with AND
    let jql = if conditions.is_empty() {
        "order by created DESC".to_string()
    } else {
        format!("{} order by created DESC", conditions.join(" AND "))
    };

    println!("Executing search with JQL: {}", jql);

    // Execute search
    let request = SearchRequest {
        jql,
        start_at: 0,
        max_results,
        fields: vec!["*all".to_string()],
        validate_query: true,
        expand: vec!["".to_string()],
    };

    let response = client.search(request).await?;

    // Handle output based on format
    match output.as_str() {
        "console" => {
            println!("\nFound {} issues:", response.total);
            for issue in response.issues {
                println!("\n{}: {}", 
                    issue.key, 
                    issue.fields.summary.unwrap_or_else(|| "No summary".to_string())
                );
                if let Some(desc) = issue.fields.description {
                    println!("Description: {}", desc);
                }
                if !issue.fields.components.is_empty() {
                    println!("Components: {}", issue.fields.components
                        .iter()
                        .map(|c| c.name.clone())
                        .collect::<Vec<_>>()
                        .join(", ")
                    );
                }
                if !issue.fields.labels.is_empty() {
                    println!("Labels: {}", issue.fields.labels.join(", "));
                }
                println!("Status: {}", issue.fields.status.name.unwrap_or_else(|| "Unknown".to_string()));
            }
        },
        "file" => {
            // Ensure cache directory exists
            if !cache_dir.exists() {
                std::fs::create_dir_all(&cache_dir)?;
            }

            let output_file = cache_dir.join("jira_search_results.json");
            std::fs::write(&output_file, serde_json::to_string_pretty(&response)?)?;
            println!("Search results written to: {}", output_file.display());
        },
        _ => return Err(anyhow::anyhow!("Invalid output format. Must be 'console' or 'file'")),
    }

    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    // Load environment variables
    dotenv().ok();

    // Initialize Jira client
    let client = JiraClient::new(
        env::var("JIRA_EMAIL").map_err(|_| anyhow::anyhow!("JIRA_EMAIL environment variable not set - did you source the .env file?"))?,
        env::var("JIRA_API_TOKEN").map_err(|_| anyhow::anyhow!("JIRA_API_TOKEN environment variable not set - did you source the .env file?"))?,
        env::var("JIRA_BASE_URL").map_err(|_| anyhow::anyhow!("JIRA_BASE_URL environment variable not set - did you source the .env file?"))?,
    )?;

    let cli = Cli::parse();

    match cli.command {
        Commands::Sprint { 
            repo, 
            cache_dir,
            development_dir,
            ticket, 
            pr_load_count, 
            minimum_similarity_score,
            additional_instructions,
        } => {
            // Try to get development directory from argument or environment
            let development_dir = development_dir.or_else(|| {
                env::var("DEVELOPMENT_DIR").ok().map(PathBuf::from)
            }).ok_or_else(|| {
                anyhow::anyhow!("Development directory must be provided either via --development-dir argument or DEVELOPMENT_DIR environment variable")
            })?;

            // Combine development directory with repo name to get full repo path
            let repo_path = development_dir.join(&repo);
            
            // Validate and prepare git repository
            prepare_git_repository(&repo_path)?;

            process_sprint_ticket(
                &client,
                repo,
                cache_dir,
                development_dir,
                ticket,
                pr_load_count,
                minimum_similarity_score,
                additional_instructions,
            ).await?;
        },
        Commands::Search {
            keywords,
            components,
            labels,
            projects,
            output,
            cache_dir,
            max_results,
        } => {
            process_search(
                &client,
                keywords,
                components,
                labels,
                projects,
                output,
                cache_dir,
                max_results,
            ).await?;
        }
    }

    Ok(())
}
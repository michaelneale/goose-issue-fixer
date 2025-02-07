use anyhow::Result;
use clap::Parser;
use pr_similarity_search::PRSearchIndex;
use std::path::PathBuf;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    /// GitHub repository owner
    #[arg(short, long)]
    owner: String,

    /// GitHub repository name
    #[arg(short, long)]
    repo: String,

    /// Cache directory for PR data
    #[arg(short, long, default_value = ".pr_cache")]
    cache_dir: PathBuf,

    /// Search query
    #[arg(short, long)]
    query: Option<String>,

    /// Number of PRs to fetch (max 100)
    #[arg(short, long, default_value = "100")]
    limit: usize,

    /// Number of results to show
    #[arg(short = 'n', long, default_value = "5")]
    num_results: usize,

    /// Force refresh of PR cache
    #[arg(short, long)]
    force_refresh: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Create cache directory if it doesn't exist
    if !cli.cache_dir.exists() {
        std::fs::create_dir_all(&cli.cache_dir)?;
    }

    // Initialize search index
    let mut search_index = PRSearchIndex::new(cli.owner, cli.repo)?;

    // Check if we need to refresh the cache
    let cache_file = cli.cache_dir.join("prs.json");
    let should_refresh = cli.force_refresh || !cache_file.exists();

    if should_refresh {
        println!("Loading {} recent PRs from GitHub...", cli.limit);
        let prs = search_index.load_recent_prs(cli.limit).await?;
        println!("Loaded {} PRs", prs.len());
    }

    // If a query is provided, perform the search
    if let Some(query) = cli.query {
        println!("\nSearching for PRs matching: {}", query);
        println!("----------------------------------------");

        let results = search_index.search(&query, cli.num_results)?;
        
        if results.is_empty() {
            println!("No matching PRs found.");
        } else {
            for result in results {
                println!("\nPR #{}: {} (Score: {:.3})", result.pr_number, result.title, result.score);
                println!("Status: {} (Checks: {})", result.status, result.checks_status);
                println!("Modified files:");
                for file in result.files.iter().take(5) {
                    println!("  - {}", file);
                }
                if result.files.len() > 5 {
                    println!("  ... and {} more files", result.files.len() - 5);
                }
            }
        }
    } else {
        println!("No search query provided. Use --query to search for similar PRs.");
    }

    Ok(())
}
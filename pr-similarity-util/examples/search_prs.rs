use anyhow::Result;
use pr_similarity_search::PRSearchIndex;
use std::collections::HashMap;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize the search index with the repository details
    let mut search_index = PRSearchIndex::new("block".to_string(), "goose".to_string())?;
    
    println!("Loading the last 100 PRs...");
    let loaded_prs = search_index.load_recent_prs(100).await?;
    
    // Count PRs by state
    let mut state_counts = HashMap::new();
    for pr in &loaded_prs {
        *state_counts.entry(pr.state.clone()).or_insert(0) += 1;
    }
    
    println!("\nLoaded {} PRs:", loaded_prs.len());
    for (state, count) in state_counts {
        println!("  {}: {}", state, count);
    }
    println!();

    // Example search queries
    let queries = [
        "tool calling",
        "authentication",
        "bug fix",
        "performance improvement",
    ];

    for query in queries {
        println!("\nSearching for PRs related to: {}", query);
        println!("----------------------------------------");

        let results = search_index.search(query, 3)?;
        
        if results.is_empty() {
            println!("No matching PRs found.");
            continue;
        }

        for result in results {
            println!("PR #{}: {} (Score: {:.3})", result.pr_number, result.title, result.score);
            println!("Status: {} (Checks: {})", result.status, result.checks_status);
            println!("Modified files:");
            for file in result.files {
                println!("  - {}", file);
            }
            println!("----------------------------------------");
        }
    }

    Ok(())
}
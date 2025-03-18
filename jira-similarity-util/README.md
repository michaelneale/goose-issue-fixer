# Jira Similarity Utility

A command-line tool focused on solving a JIRA ticket by analyzing similar Jira issues, prepare context for LLM processing, and automating creating a Pull Request.

## Environment Variables

The following environment variables must be set either in your shell or in a `.env` file:

```bash
# Jira Authentication
JIRA_EMAIL=your.email@domain.com
JIRA_API_TOKEN=your_jira_api_token
JIRA_BASE_URL=https://your-org.atlassian.net

# Development Settings
DEVELOPMENT_DIR=/path/to/your/development/directory  # Parent directory containing git repositories
```

To use a `.env` file:
1. Create a `.env` file in the project directory
2. Add the required variables
3. Either:
   - Source it directly: `source .env`
   - Let the tool load it automatically when run

## Commands

### Sprint Command

The `sprint` command helps prepare for fixing a Jira issue.
The command will:
1. Find similar Jira issues based on various criteria
2. Calculate similarity scores
3. Prepare the git repository (fetch latest, switch to main branch)
4. Generate instruction files for LLM processing
5. Run the goose LLM tool with the generated instructions

```bash
# Basic usage
jira-similarity-util sprint --repo <repository-name> --ticket <JIRA-123>

# Full options
jira-similarity-util sprint \
    --repo <repository-name> \
    --ticket <JIRA-123> \
    --cache-dir /tmp/.jira_cache \
    --development-dir /path/to/dev/dir \
    --pr-load-count 10 \
    --minimum-similarity-score 1.0 \
    --additional-instructions path/to/extra.txt

# Example
jira-similarity-util sprint --repo your-repo-name --ticket JIRA-123
```

Options:
- `--repo, -r`: Name of the GitHub repository
- `--ticket, -t`: Jira ticket key (e.g., JIRA-123)
- `--cache-dir, -c`: Directory for caching data (default: /tmp/.jira_cache)
- `--development-dir, -d`: Directory containing git repositories (can also use DEVELOPMENT_DIR env var)
- `--pr-load-count, -n`: Number of similar PRs to fetch (default: 10)
- `--minimum-similarity-score, -s`: Minimum similarity threshold (default: 1.0)
- `--additional-instructions, -a`: Optional file containing additional instructions to provide to the LLM. Useful for repository-specific debug / iteration instrutions

### Search Command

The `search` command allows searching Jira issues with various filters and outputs the results either to the console or a JSON file.

```bash
# Basic search by keywords
jira-similarity-util search --keywords "authentication"

# Search with multiple filters
jira-similarity-util search \
    --keywords "login" \
    --components "backend,api" \
    --labels "security" \
    --projects "PROJ1,PROJ2" \
    --output console \
    --max-results 50

# Example: Search for iOS issues
jira-similarity-util search \
    --components "iOS" \
    --projects "PROJ1" \
    --output console
```

Options:
- `--keywords, -k`: Search in issue summary and description
- `--components, -c`: Filter by components (comma-separated)
- `--labels, -l`: Filter by labels (comma-separated)
- `--projects, -p`: Filter by projects (comma-separated)
- `--output, -o`: Output format: 'console' or 'file' (default: file)
- `--cache-dir, -c`: Directory for output files (default: .jira_cache)
- `--max-results, -n`: Maximum number of results to return (default: 50)

## Output Files

The tool generates several files in the cache directory:

**Sprint Command:**
```
.jira_cache/
└── JIRA-123/
    ├── instructions.txt          # Instructions for the LLM
    ├── llm_context_issue_summary.md  # Context about similar issues
    └── raw_issues.json          # Raw Jira issue data
```

**Search Command** (when using file output):
```
.jira_cache/
└── jira_search_results.json     # Search results in JSON format
```

## Development

Built with Rust using:
- `clap` for CLI argument parsing
- `reqwest` for HTTP requests
- `git2` for git operations
- `serde` for JSON handling
- `tokio` for async operations

To build from source:
```bash
cargo build --release
```

The binary will be available at `target/release/jira-similarity-util`

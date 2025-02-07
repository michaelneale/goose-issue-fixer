#!/bin/bash

# Function to extract job ID from a GitHub Actions URL
extract_job_id() {
    local url=$1
    echo "$url" | grep -o '/job/[0-9]*' | grep -o '[0-9]*'
}

# Function to extract repo from GitHub URL
extract_repo() {
    local url=$1
    echo "$url" | sed -E 's|https://github.com/([^/]+/[^/]+)/.*|\1|'
}

# Show how we get CI logs
show_ci_logs() {
    local pr_url=$1
    
    # Check PR status and capture the output
    checks_output=$(gh pr checks "$pr_url" 2>&1)
    
    # Check if there are no checks
    if echo "$checks_output" | grep -q "no checks"; then
        echo "$checks_output"
        exit 0
    fi
    
    # Check if there are any failing checks
    if echo "$checks_output" | grep -q "fail\|error"; then
        echo "Found failing checks:"
        echo "----------------------------------------"
        echo "$checks_output"
        echo "----------------------------------------"
        
        echo -e "\nTo get detailed logs for a failed job:"
        echo "1. Copy the failed job URL from above"
        echo "2. Extract the job ID (the number after /job/)"
        echo "3. Run: gh run view --log --job <job-id> -R <repository>"
        exit 1
    fi
    
    # Check if there are any pending or running checks
    if echo "$checks_output" | grep -q "pending\|in_progress\|queued\|running"; then
        echo "Some checks are still running:"
        echo "----------------------------------------"
        echo "$checks_output"
        echo "----------------------------------------"
        exit 2
    fi
    
    # If we get here, all checks have passed
    echo "ok - all checks passing"
    exit 0
}

# If no arguments provided, show usage
if [ $# -lt 1 ]; then
    echo "Usage: $0 <pr-url>"
    echo "Example: $0 https://github.com/block/goose/pull/1033"
    exit 1
fi

PR_URL=$1

# Show the process of getting CI logs
show_ci_logs "$PR_URL"
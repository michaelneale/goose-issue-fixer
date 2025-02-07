#!/bin/bash

# Function to extract repo from GitHub URL
extract_repo() {
    local url=$1
    echo "$url" | sed -E 's|https://github.com/([^/]+/[^/]+)/.*|\1|'
}

# Function to extract issue number from GitHub URL
extract_issue_number() {
    local url=$1
    echo "$url" | grep -o '/issues/[0-9]*' | grep -o '[0-9]*'
}

# Show issue information
show_issue_info() {
    local issue_url=$1
    local repo=$(extract_repo "$issue_url")
    local issue_number=$(extract_issue_number "$issue_url")
    
    echo "Fetching information for issue #$issue_number from $repo"
    echo "----------------------------------------"
    
    echo -e "\nIssue Details:"
    echo "----------------------------------------"
    gh issue view "$issue_number" -R "$repo"
    echo "----------------------------------------"
}

# If no arguments provided, show usage
if [ $# -lt 1 ]; then
    echo "Usage: $0 <issue-url>"
    echo "Example: $0 https://github.com/block/goose/issues/1022"
    exit 1
fi

ISSUE_URL=$1

# Show the issue information
show_issue_info "$ISSUE_URL"
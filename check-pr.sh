#!/bin/bash

# Check if PR URL is provided
if [ -z "$1" ]; then
    echo "Usage: $0 <pr-url>"
    echo "Example: $0 https://github.com/owner/repo/pull/123"
    exit 1
fi

PR_URL=$1

# Extract repo and PR number from URL
if [[ $PR_URL =~ github\.com/([^/]+/[^/]+)/pull/([0-9]+) ]]; then
    REPO="${BASH_REMATCH[1]}"
    PR_NUMBER="${BASH_REMATCH[2]}"
else
    echo "Invalid GitHub PR URL format. Expected: https://github.com/owner/repo/pull/number"
    exit 1
fi

# Get the last commit information from the PR
last_commit_info=$(gh pr view $PR_NUMBER --repo $REPO --json commits --jq '.commits[-1]')
last_commit_date=$(echo "$last_commit_info" | jq -r '.committedDate')

echo "Checking PR #$PR_NUMBER in repository $REPO"
echo "Last commit was at: $last_commit_date"
echo ""
echo "🔍 CHECKING STATUS CHECKS..."
echo "----------------------------------------"

# Get and display check status
checks=$(gh pr view $PR_NUMBER --repo $REPO --json statusCheckRollup)
echo "$checks" | jq -r '.statusCheckRollup[] | "Check: \(.name)\nStatus: \(.status)\nConclusion: \(.conclusion // "pending")\nDetails: \(.detailsUrl)\n"'

echo ""
echo "🔍 LOOKING FOR @GOOSE MENTIONS..."
echo "----------------------------------------"

# Get all types of comments (review comments, issue comments, and review thread comments)
review_comments=$(gh api "/repos/$REPO/pulls/$PR_NUMBER/comments")
issue_comments=$(gh api "/repos/$REPO/issues/$PR_NUMBER/comments")

# Process review comments
echo "Review Comments after last commit ($last_commit_date):"
echo "$review_comments" | jq -r --arg date "$last_commit_date" '
    .[] | 
    select(.body | contains("@goose")) |
    select(.created_at > $date) |
    "Found @goose mention:\n\nFile: \(.path)\nLine: \(.line)\nComment: \(.body)\nComment Date: \(.created_at)\n\nContext:\n\(.diff_hunk)\n-------------------"
'

# Process issue comments
echo "Issue Comments after last commit ($last_commit_date):"
echo "$issue_comments" | jq -r --arg date "$last_commit_date" '
    .[] | 
    select(.body | contains("@goose")) |
    select(.created_at > $date) |
    "Found @goose mention:\n\nComment: \(.body)\nComment Date: \(.created_at)\n-------------------"
'

# If no comments were found with @goose after the last commit
if [ -z "$(echo "$review_comments" | jq -r --arg date "$last_commit_date" '.[] | select(.body | contains("@goose")) | select(.created_at > $date)')" ] && \
   [ -z "$(echo "$issue_comments" | jq -r --arg date "$last_commit_date" '.[] | select(.body | contains("@goose")) | select(.created_at > $date)')" ]; then
    echo "No comments containing '@goose' were found in PR #$PR_NUMBER after the last commit"
fi
#!/bin/bash

# Function to format status with color and symbols
format_status() {
    local status=$1
    status=$(echo "$status" | tr '[:upper:]' '[:lower:]')
    case "$status" in
        "pass"|"success") echo "✅ PASSED";;
        "fail"|"failure") echo "❌ FAILED";;
        "pending") echo "⏳ PENDING";;
        "running") echo "🔄 RUNNING";;
        *) echo "ℹ️  $(echo "$status" | tr '[:lower:]' '[:upper:]')";;
    esac
}

# If no arguments provided, show usage
if [ $# -lt 1 ]; then
    echo "Usage: $0 <pr-url>"
    echo "Example: $0 https://github.com/owner/reponame/pull/1033"
    exit 1
fi

PR_URL=$1

# Extract repo from URL
if [[ $PR_URL =~ github\.com/([^/]+/[^/]+)/pull/([0-9]+) ]]; then
    REPO="${BASH_REMATCH[1]}"
    PR_NUMBER="${BASH_REMATCH[2]}"
else
    echo "Invalid GitHub PR URL format"
    exit 1
fi

echo "🔍 CI STATUS FOR PR #$PR_NUMBER IN $REPO"
echo "Last checked: $(date)"
echo "----------------------------------------"

# Get checks status
checks_output=$(gh pr checks "$PR_URL" 2>&1)

# Check if there are no checks
if echo "$checks_output" | grep -q "no checks"; then
    echo "STATUS: No checks found for this PR"
    exit 0
fi

# Process each check
total_checks=0
failed_checks=0
pending_checks=0
passed_checks=0

echo "CHECKS SUMMARY:"
echo ""

while IFS= read -r line; do
    if [[ -n $line ]]; then
        # Skip non-check lines
        if [[ $line =~ "To get detailed logs" ]] || [[ $line =~ "Copy the failed" ]] || [[ $line =~ "Extract the job" ]] || [[ $line =~ "Run: gh" ]]; then
            continue
        fi
        
        # Extract check information
        name=$(echo "$line" | awk -F'\t' '{print $1}')
        status=$(echo "$line" | awk -F'\t' '{print $2}')
        duration=$(echo "$line" | awk -F'\t' '{print $3}')
        url=$(echo "$line" | awk -F'\t' '{print $4}')
        
        # Skip empty lines
        if [[ -z $name ]] || [[ -z $status ]]; then
            continue
        fi
        
        formatted_status=$(format_status "$status")
        
        # Update counters
        ((total_checks++))
        case "$status" in
            fail|failure) ((failed_checks++));;
            pending|in_progress|queued|running) ((pending_checks++));;
            pass|success) ((passed_checks++));;
        esac
        
        # Display check details
        echo "Check: $name"
        echo "Status: $formatted_status"
        echo "Duration: $duration"
        if [[ $status == "fail" || $status == "failure" ]]; then
            echo "Log Command: gh run view --log --job $(echo "$url" | grep -o '/job/[0-9]*' | grep -o '[0-9]*') -R $REPO"
        fi
        echo "Details: $url"
        echo "----------------------------------------"
    fi
done <<< "$checks_output"

echo ""
echo "OVERALL STATUS:"
echo "Total Checks: $total_checks"
echo "✅ Passed: $passed_checks"
echo "❌ Failed: $failed_checks"
echo "⏳ Pending: $pending_checks"
echo ""

if [ "$failed_checks" -gt 0 ]; then
    echo "❌ FINAL STATUS: CHECKS FAILING"
    exit 1
elif [ "$pending_checks" -gt 0 ]; then
    echo "⏳ FINAL STATUS: CHECKS IN PROGRESS"
    exit 2
else
    echo "✅ FINAL STATUS: ALL CHECKS PASSING"
    exit 0
fi
#!/bin/bash

# Define constants
ISSUE_CONTENT_FILE="issue_content.json"
SEPARATOR="----------------------------------------"

# Check if jq is installed
if ! command -v jq &> /dev/null; then
    echo "Error: jq is required but not installed. Please install jq to continue."
    exit 1
fi

# Function to display issue content
display_issue_content() {
    local file_path="$1"
    
    echo "Using existing issue content from $file_path"
    echo "$SEPARATOR"
    
    # Determine if it's GitHub or JIRA content by checking the file structure
    if grep -q "\"fields\":" "$file_path"; then
        # It's JIRA content
        echo -e "\nJIRA Details:"
        echo "$SEPARATOR"
        jq -r '.fields.summary, "", .fields.description' "$file_path"
    else
        # It's GitHub content
        echo -e "\nGitHub Issue Details:"
        echo "$SEPARATOR"
        jq -r '.title, "", .body' "$file_path"
    fi
    echo "$SEPARATOR"
}

# Function to find issue content file
find_issue_content() {
    # Check locations in priority order
    local possible_locations=(
        "$ISSUE_CONTENT_FILE"
        "../$ISSUE_CONTENT_FILE"
        "$(pwd)/$ISSUE_CONTENT_FILE"
        "$(dirname "$(pwd)")/$ISSUE_CONTENT_FILE"
    )
    
    for location in "${possible_locations[@]}"; do
        if [ -f "$location" ]; then
            echo "$location"
            return 0
        fi
    done
    
    echo ""
    return 1
}

# Main execution
main() {
    local file_path=$(find_issue_content)
    
    if [ -n "$file_path" ]; then
        display_issue_content "$file_path"
    else
        echo "No issue_content.json found"
        echo "Note: This file is created by the 'solve' script when processing a ticket."
    fi
}

# Run the main function
main
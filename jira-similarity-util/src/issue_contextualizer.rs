use crate::jira_client::Issue;

pub struct IssueContextualizer {}

impl IssueContextualizer {
    pub fn sort_issues_by_similarity<'a>(
        issue_to_solve: &Issue, 
        related_issues: Vec<&'a Issue>
    ) -> Vec<(&'a Issue, f32)> {
        let mut scored_issues: Vec<(&Issue, f32)> = related_issues.into_iter()
            .map(|issue| {
                let score = Self::calculate_similarity_score(&issue_to_solve, &issue);
                (issue, score)
            })
            .collect();

        // Sort by score in descending order
        scored_issues.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        scored_issues
    }

    /// Get formatted context about an issue and its similar issues for the LLM
    pub fn get_llm_context(issue_to_solve: &Issue, related_issues_by_similarity: Vec<(Issue, f32)>) -> String {
        // Format the context
        let context = format!(
            r#"# Primary Issue
Key: {key}
Title: {title}
Description:
{description}

# Additional Context
- Project: {project}
- Component(s): {components}
- Labels: {labels}

# Similar Issues (by relevance score)

{similar_issues}
"#,
            key = issue_to_solve.key,
            title = issue_to_solve.fields.summary.clone().unwrap_or("No Title".to_string()),
            description = issue_to_solve.fields.description.clone().unwrap_or("No Description".to_string()),
            similar_issues = Self::format_similar_issues(issue_to_solve, &related_issues_by_similarity),
            project = issue_to_solve.fields.project.key,
            components = issue_to_solve.fields.components.iter()
                .map(|c| &c.name)
                .map(String::as_str)
                .collect::<Vec<_>>()
                .join(", "),
            labels = issue_to_solve.fields.labels.join(", ")
        );

        context
    }

    /// Format similar issues for context
    fn format_similar_issues(original: &Issue, scored_issues: &Vec<(Issue, f32)>) -> String {
        scored_issues.iter()
            .take(5)  // Limit to most relevant
            .map(|(issue, score)| format!(
                r#"## {key} (Similarity: {score:.2})
Title: {title}
Resolution: {resolution}
Reason for similarity: {reasons}
Status: {status}
{pull_requests}
{components}
{labels}
Description:
{description}

"#,
                key = issue.key,
                score = score,
                title = issue.fields.summary.clone().unwrap_or("No Title".to_string()),
                resolution = issue.fields.resolution.as_ref()
                    .map_or("Unresolved".to_string(), |r| r.name.clone()),
                reasons = Self::get_similarity_reasons(original, issue),
                status = issue.fields.status.name.as_deref().unwrap_or("Unknown"),
                pull_requests = if !issue.pull_requests.is_empty() { 
                    format!("Pull Requests: {}", issue.pull_requests.iter()
                        .map(|pr| &pr.url)
                        .map(String::as_str)
                        .collect::<Vec<_>>()
                        .join(", "))
                } else {
                    "Pull Requests:".to_string()
                },
                components = if !issue.fields.components.is_empty() {
                    format!("Components: {}", issue.fields.components.iter()
                        .map(|c| &c.name)
                        .map(String::as_str)
                        .collect::<Vec<_>>()
                        .join(", "))
                } else {
                    "Components:".to_string()
                },
                labels = if !issue.fields.labels.is_empty() {
                    format!("Labels: {}", issue.fields.labels.join(", "))
                } else {
                    "Labels:".to_string()
                },
                description = issue.fields.description.clone().unwrap_or("No Description".to_string()),
            ))
            .collect::<Vec<_>>()
            .join("\n\n")
    }

    /// Get reasons why an issue is similar to the original
    fn get_similarity_reasons(original: &Issue, other: &Issue) -> String {
        let mut reasons = Vec::new();

        // Check for linked issues (highest priority)
        if original.fields.issue_links.as_ref().map_or(false, |links| {
            links.iter().any(|link| {
                link.inward_issue.as_ref().map_or(false, |i| i.key == other.key)
            })
        }) {
            reasons.push("Directly linked issue".to_string());
        }

        // Check for same epic
        if original.fields.epic.as_ref().map_or(false, |e| {
            other.fields.epic.as_ref().map_or(false, |oe| e.id == oe.id)
        }) {
            reasons.push("Same epic".to_string());
        }

        // Check for parent relationships
        if let Some(parent) = &original.fields.parent {
            if other.key == parent.key {
                reasons.push("This is the parent ticket".to_string());
            } else if other.fields.parent.as_ref().map_or(false, |op| op.key == parent.key) {
                reasons.push("Shares same parent".to_string());
            }
        }

        // Check for shared components (only if same project)
        if original.fields.project.key == other.fields.project.key {
            let original_components: std::collections::HashSet<_> = original.fields.components.iter()
                .map(|c| &c.name)
                .collect();
            let other_components: std::collections::HashSet<_> = other.fields.components.iter()
                .map(|c| &c.name)
                .collect();
            let shared_components: Vec<_> = original_components.intersection(&other_components)
                .map(|&s| s.to_string())
                .collect();
            if !shared_components.is_empty() {
                reasons.push(format!("Shared components: {}", shared_components.join(", ")));
            }
        }

        // Check for shared labels
        let original_labels: std::collections::HashSet<_> = original.fields.labels.iter().collect();
        let other_labels: std::collections::HashSet<_> = other.fields.labels.iter().collect();
        let shared_labels: Vec<_> = original_labels.intersection(&other_labels)
            .map(|s| s.to_string())
            .collect();
        if !shared_labels.is_empty() {
            reasons.push(format!("Shared labels: {}", shared_labels.join(", ")));
        }

        // If no specific reasons found
        if reasons.is_empty() {
            return "Similar based on content and description".to_string();
        }

        reasons.join("; ")
    }

    /// Calculate a similarity score between two issues
    ///
    /// Extremely basic; Could use much better heuristics
    ///
    /// Order of importance:
    /// - Linked Issues
    /// - Issues in the same epic
    /// - Issues with the same parent
    /// - Issues with the same component
    /// - Issues with the same labels
    /// - Issues on the same board
    /// TODO: Probably keyword similarity / general rework entirely
    fn calculate_similarity_score(original: &Issue, other: &Issue) -> f32 {
        let mut score = 0.0;
        
        // Cross-project relationships
        
        // Linked issues (highest weight)
        if original.fields.issue_links.as_ref().map_or(false, |links| {
            links.iter().any(|link| {
                link.inward_issue.as_ref().map_or(false, |i| i.key == other.key)
            })
        }) {
            score += 5.0;
        }

        // Same epic
        if original.fields.epic.as_ref().map_or(false, |e| {
            other.fields.epic.as_ref().map_or(false, |oe| e.id == oe.id)
        }) {
            score += 4.0;
        }

        // Parent relationships
        if let Some(parent) = &original.fields.parent {
            if other.key == parent.key {
                // This is the parent ticket
                score += 3.5;
            } else if other.fields.parent.as_ref().map_or(false, |op| op.key == parent.key) {
                // This is a sibling ticket
                score += 3.0;
            }
        }

        // Project-scoped relationships (only if same project)
        if original.fields.project.key == other.fields.project.key {
            // Component overlap
            let original_components: std::collections::HashSet<_> = original.fields.components.iter()
                .map(|c| &c.id)
                .collect();
            let other_components: std::collections::HashSet<_> = other.fields.components.iter()
                .map(|c| &c.id)
                .collect();
            let component_overlap = original_components.intersection(&other_components).count();
            score += (component_overlap as f32) * 0.5;

            // Label overlap
            let original_labels: std::collections::HashSet<_> = original.fields.labels.iter().collect();
            let other_labels: std::collections::HashSet<_> = other.fields.labels.iter().collect();
            let label_overlap = original_labels.intersection(&other_labels).count();
            score += (label_overlap as f32) * 0.3;
        }

        score
    }
}

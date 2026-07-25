---
description: Review PR comments and work on thumbs-up marked items
allowed-tools: Bash(gh:*)
---

Review comments on the current PR and work on comments marked with a thumbs up.

## Instructions

1. **Get the current PR**:

   - Run `gh pr view --json number,url` to get the current branch's PR
   - If no PR exists, inform the user

2. **Fetch PR comments**:

   - Get review comments: `gh pr view --json reviews,comments`
   - Get inline/diff comments:
     `gh api repos/{owner}/{repo}/pulls/{pr_number}/comments`

3. **Identify actionable comments**:

   - **Skip resolved comments** — filter out any comment threads that are
     already resolved (check for `"isResolved": true` or equivalent in the API
     response)
   - Look for comments with a thumbs up reaction (`:+1:` or `👍`)
   - To check reactions:
     `gh api repos/{owner}/{repo}/pulls/comments/{comment_id}/reactions`
   - A thumbs up indicates the user wants you to address this comment

4. **For each thumbs-up comment**:

   - Show the comment content and file location
   - Understand what change is being requested
   - Implement the requested change
   - Mark progress as you go

5. **Summary**:

   - List all comments found
   - Indicate which had thumbs up and were worked on
   - Note any comments that were unclear or couldn't be addressed

## Useful gh commands

```bash
# Get PR number for current branch
gh pr view --json number -q '.number'

# Get repo owner/name
gh repo view --json nameWithOwner -q '.nameWithOwner'

# Get all review comments with details (includes resolved threads via GraphQL)
gh api graphql -f query='query { repository(owner:"{owner}", name:"{repo}") { pullRequest(number:{number}) { reviewThreads(first:100) { nodes { isResolved comments(first:10) { nodes { id databaseId body path line author { login } } } } } } } }'

# Fallback: REST API for review comments (no resolved status)
gh api repos/{owner}/{repo}/pulls/{number}/comments

# Check reactions on a comment
gh api repos/{owner}/{repo}/pulls/comments/{comment_id}/reactions
```

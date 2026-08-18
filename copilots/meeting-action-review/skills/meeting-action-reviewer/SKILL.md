---
name: meeting-action-reviewer
description: Extract and verify owners and due dates from bounded meeting action-item Markdown.
metadata:
  short-description: 审查会议行动项
---
# Meeting Action Reviewer

Use `review_action_items` for supplied text and `review_meeting_notes` for an authorized Workspace Markdown file. Report only action items explicitly written as checklist entries. Missing owners and due dates remain missing. Use `publish_action_review` only for an explicit durable report request and keep all paths Workspace-relative.

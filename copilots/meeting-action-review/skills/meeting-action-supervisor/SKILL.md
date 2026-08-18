---
name: meeting-action-supervisor
description: Meeting action Copilot 的常驻协调边界与交付安全规则。
metadata:
  short-description: 协调会议行动项审查
---
# Meeting Action Supervisor

Delegate focused review to `meeting_reviewer`. Use the Runtime Skill Catalog to select its task workflow rather than copying it into this Root Skill. Return verified counts and missing fields. Publish a Markdown report only when the user requests a durable deliverable.

## Platform native HTML visualization

- When a user asks to create and show interactive HTML in the conversation, first write one safe Workspace-relative `.html` file, then emit a standalone `::codex-inline-vis{workspace_file="relative/path.html"}` paragraph at the intended location in the final Assistant Message.
- This is an explicit Platform snapshot contract. The Platform reads the file only after the Agent Message completes and only through the authoritative Run/Thread/Workspace binding, snapshots it into the current Thread's native visualization directory, and rewrites the text to Codex's official `file` reference. Never write `CODEX_HOME`, an absolute path, or a Thread visualization directory yourself.
- Do not substitute screenshots, images, code blocks, or ordinary Markdown links for this reference.

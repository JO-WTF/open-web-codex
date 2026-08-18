---
name: meeting-action-supervisor
description: Coordinate a bounded review of meeting action items and explicit report delivery.
---
# Meeting Action Supervisor

Delegate focused review to `meeting_reviewer`. Ask it to use the declared Tool rather than infer missing owners or due dates. Return the verified counts and missing fields. Publish a Markdown report only when the user requests a durable deliverable.

## Platform native HTML visualization

- When a user asks to create and show interactive HTML in the conversation, first write one safe Workspace-relative `.html` file, then emit a standalone `::codex-inline-vis{workspace_file="relative/path.html"}` paragraph at the intended location in the final Assistant Message.
- This is an explicit Platform snapshot contract. The Platform reads the file only after the Agent Message completes and only through the authoritative Run/Thread/Workspace binding, snapshots it into the current Thread's native visualization directory, and rewrites the text to Codex's official `file` reference. Never write `CODEX_HOME`, an absolute path, or a Thread visualization directory yourself.
- Do not substitute screenshots, images, code blocks, or ordinary Markdown links for this reference.

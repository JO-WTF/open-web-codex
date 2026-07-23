#!/usr/bin/env bash
set -euo pipefail
repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
npm --prefix "$repo_root/apps/web" test -- \
  src/utils/replyCards.test.ts \
  src/components/Conversation/messages/MapReplyCard.test.ts \
  src/components/Conversation/messages/AssistantMessage.test.tsx \
  --run

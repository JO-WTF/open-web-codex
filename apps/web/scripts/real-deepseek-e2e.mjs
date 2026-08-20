#!/usr/bin/env node

// Multi-agent real business acceptance only.  Shared provider, lifecycle,
// timeline and cleanup logic lives in real-deepseek/harness.mjs.
process.env.E2E_REAL_DEEPSEEK_ENTRY = "multi-business";
await import("./real-deepseek/harness.mjs");

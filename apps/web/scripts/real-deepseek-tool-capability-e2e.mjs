#!/usr/bin/env node

// Diagnostic-only native tool capability acceptance.  It is intentionally not
// part of either warehouse business gate.
process.env.E2E_REAL_DEEPSEEK_ENTRY = "tool-capability";
await import("./real-deepseek/harness.mjs");

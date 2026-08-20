#!/usr/bin/env node

process.env.E2E_REAL_DEEPSEEK_SCENARIO ??= "single-agent";
await import("./real-deepseek-e2e.mjs");

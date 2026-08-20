#!/usr/bin/env node

process.env.E2E_REAL_DEEPSEEK_ENTRY = "single-business";
await import("./real-deepseek/harness.mjs");

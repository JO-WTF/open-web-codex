# Generated Codex contract artifacts

Files in this directory are produced from a specific Codex build. Do not edit
them by hand.

Generate the bundle with:

```bash
npm run generate:codex-contracts
```

Verify downloads with:

```bash
npm run fetch:codex-contracts -- --source ./contracts/codex/generated/contract-bundle.v1.json --sha256 "$(cat ./contracts/codex/generated/contract-bundle.v1.sha256)"
```

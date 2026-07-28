# Capability publication sources

This directory is the repository entry point for code-managed Supervisor
Packages and governed Agent Definitions.

```text
capabilities/
├── supervisors/<policy-id>/<version>/
│   ├── manifest.json
│   ├── instructions.md
│   └── artifact-contracts.json
└── agents/<definition-id>/<version>/
    ├── definition.json
    └── instructions.md
```

It is a publication catalog, not a Runtime discovery directory:

- Codex Runtime still owns Agent execution, Skills, Plugins, MCP and Tools.
- Tool and MCP implementations stay in `tools/` or their owning packages.
- Package manifests relate Agents, Artifact contracts and Runtime capabilities
  by stable ID and version; directory nesting is not an authorization rule.
- Web-authored Supervisor Definitions, Revisions and Releases are
  organization-scoped PostgreSQL resources. They are not written here or into a
  Profile.
- Web-authored Agent Definitions, draft Revisions and immutable Releases are
  also organization-scoped PostgreSQL resources. During the pre-native-CRUD
  phase, each Release selects one exact reviewed Agent entry from this directory
  as its capability template and may narrow, but never expand, its Artifact
  contract.
- Profile `CODEX_HOME/agents` remains Runtime configuration and is not a
  substitute for governed Agent Definition publication.

The Rust owner is `apps/web/crates/supervisor-catalog`. Server routes and the
browser consume its validated typed results rather than reading these files
independently.

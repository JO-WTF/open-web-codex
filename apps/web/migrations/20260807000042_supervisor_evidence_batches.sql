-- Make the current Supervisor Draft execute dependent evidence in ordered
-- batches and carry producer references in the terminal child message. This
-- changes current authoring state only; historical snapshots and sessions are
-- intentionally not rewritten.
UPDATE supervisor_revisions revision
SET draft_spec = jsonb_set(
        revision.draft_spec,
        '{custom_instructions}',
        to_jsonb(
            revision.draft_spec->>'custom_instructions'
            || E'\n\n'
            || $supervisor_evidence_batches$
## Sequential evidence batches

Run the journey in four ordered batches. Batch 1: Network Planning publishes `data_requirement_profile.v1` and returns its complete typed handoff. Batch 2: Data Preparation reads that Profile once, discovers and inspects every authorized Workspace source, then publishes `source_profile.v1` and `mapping_proposal.v1`. Batch 3: after the required mapping and parameter decision, Data Preparation publishes and validates `planning-dataset.v2`. Batch 4: Network Planning publishes `analysis_readiness_review.v1` and performs the requested analysis. Do not profile Workspace files before Batch 1, skip a batch, run dependent batches in parallel, or repeat an unchanged batch.

Every producer Tool result is submitted as one bounded `HANDOFF_BATCH` containing the exact Artifact schema, `resource_name` and complete structured `data_ref` returned by the Tool. Copy those fields verbatim into the next assignment. A terminal child is not to be called again just to recover a missing handoff; if its final message lacks the complete batch, end the current batch with a typed missing-evidence result and report the business prerequisite. Never construct a URI, copy source rows, or replace a typed handoff with a prose summary.
$supervisor_evidence_batches$::text
        ),
        true
    ),
    revision_number = revision.revision_number + 1,
    updated_at = now()
FROM supervisor_definitions definition
WHERE revision.definition_id = definition.id
  AND revision.organization_id = definition.organization_id
  AND revision.state = 'draft'
  AND revision.version = '5.2.0'
  AND revision.draft_spec->>'version' = '5.2.0'
  AND definition.policy_id = 'enterprise-supervisor-copilot'
  AND position(
        '## Sequential evidence batches'
        IN coalesce(revision.draft_spec->>'custom_instructions', '')
      ) = 0;

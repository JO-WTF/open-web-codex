---
name: assess-decision-risk
description: Publish an evidence-linked risk_register.v1 for a supply-chain decision when material uncertainty, implementation exposure, or missing external evidence could change the recommendation.
---

# Assess Decision Risk

1. Read the exact network and financial Resources supplied by the Supervisor.
2. Separate supported risks from unsupported external claims. Missing Indonesian legal,
   tax, permit, carrier, or customer evidence remains an explicit gap.
3. For each material risk, record category, likelihood, impact, mitigation, measurable
   trigger, and at least one exact planning Resource reference.
4. Call `supply_chain_planner.publish_risk_register`, then validate the returned
   `risk_register.v1` Resource.
5. Return the unchanged `data_ref` and `resource_name`, emphasizing high-scoring
   unresolved risks and assessment limits.

Do not alter calculation Resources or convert unsupported assumptions into facts.

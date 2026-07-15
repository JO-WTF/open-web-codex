-- Scope projects to organizations for membership-based authorization.

ALTER TABLE projects
    ADD COLUMN IF NOT EXISTS organization_id UUID REFERENCES organizations(id);

UPDATE projects
SET organization_id = (
    SELECT o.id
    FROM organizations o
    ORDER BY o.created_at ASC
    LIMIT 1
)
WHERE organization_id IS NULL
  AND EXISTS (SELECT 1 FROM organizations o);

ALTER TABLE projects
    ALTER COLUMN organization_id SET NOT NULL;

CREATE INDEX IF NOT EXISTS idx_projects_organization ON projects(organization_id);

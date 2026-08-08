PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS network_cases (
    case_id TEXT PRIMARY KEY,
    workspace_fingerprint TEXT NOT NULL,
    country_code TEXT NOT NULL CHECK (length(country_code) = 2),
    intent_sha256 TEXT NOT NULL CHECK (length(intent_sha256) = 64),
    state TEXT NOT NULL CHECK (state IN ('active', 'completed', 'failed', 'archived')),
    revision INTEGER NOT NULL CHECK (revision >= 1),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    last_accessed_at TEXT NOT NULL,
    archived_at TEXT
);

CREATE TABLE IF NOT EXISTS case_components (
    component_id TEXT PRIMARY KEY,
    case_id TEXT NOT NULL REFERENCES network_cases(case_id) ON DELETE CASCADE,
    component_kind TEXT NOT NULL,
    component_revision INTEGER NOT NULL CHECK (component_revision >= 1),
    content_sha256 TEXT NOT NULL CHECK (length(content_sha256) = 64),
    row_count INTEGER CHECK (row_count IS NULL OR row_count >= 0),
    metadata_json TEXT NOT NULL,
    created_at TEXT NOT NULL,
    UNIQUE(case_id, component_kind, component_revision),
    UNIQUE(case_id, component_kind, content_sha256)
);

CREATE TABLE IF NOT EXISTS case_component_dependencies (
    component_id TEXT NOT NULL REFERENCES case_components(component_id) ON DELETE CASCADE,
    depends_on_component_id TEXT NOT NULL REFERENCES case_components(component_id) ON DELETE CASCADE,
    PRIMARY KEY(component_id, depends_on_component_id),
    CHECK(component_id <> depends_on_component_id)
);

CREATE TABLE IF NOT EXISTS case_bindings (
    case_id TEXT NOT NULL REFERENCES network_cases(case_id) ON DELETE CASCADE,
    facet TEXT NOT NULL,
    component_id TEXT REFERENCES case_components(component_id) ON DELETE SET NULL,
    state TEXT NOT NULL CHECK (state IN (
        'not_required', 'missing', 'needs_input', 'ready', 'stale', 'unavailable', 'failed'
    )),
    issue_summary_json TEXT NOT NULL DEFAULT '[]',
    updated_at TEXT NOT NULL,
    PRIMARY KEY(case_id, facet)
);

CREATE TABLE IF NOT EXISTS case_operations (
    operation_id TEXT PRIMARY KEY,
    case_id TEXT NOT NULL REFERENCES network_cases(case_id) ON DELETE CASCADE,
    operation_kind TEXT NOT NULL,
    input_sha256 TEXT NOT NULL CHECK (length(input_sha256) = 64),
    state TEXT NOT NULL CHECK (state IN (
        'running', 'completed', 'failed', 'interrupted', 'timeout'
    )),
    result_component_id TEXT REFERENCES case_components(component_id) ON DELETE SET NULL,
    error_code TEXT,
    safe_error_message TEXT,
    started_at TEXT NOT NULL,
    terminal_at TEXT,
    UNIQUE(case_id, operation_kind, input_sha256)
);

CREATE TABLE IF NOT EXISTS case_sources (
    source_id TEXT PRIMARY KEY,
    case_id TEXT NOT NULL REFERENCES network_cases(case_id) ON DELETE CASCADE,
    source_ref TEXT NOT NULL,
    display_name TEXT NOT NULL,
    media_type TEXT NOT NULL,
    content_sha256 TEXT NOT NULL CHECK (length(content_sha256) = 64),
    byte_size INTEGER NOT NULL CHECK (byte_size >= 0),
    row_count INTEGER CHECK (row_count IS NULL OR row_count >= 0),
    source_revision INTEGER NOT NULL CHECK (source_revision >= 1),
    active INTEGER NOT NULL CHECK (active IN (0, 1)),
    metadata_json TEXT NOT NULL,
    created_at TEXT NOT NULL,
    UNIQUE(case_id, source_ref, source_revision),
    UNIQUE(case_id, source_ref, content_sha256)
);

CREATE TABLE IF NOT EXISTS case_requirements (
    component_id TEXT PRIMARY KEY REFERENCES case_components(component_id) ON DELETE CASCADE,
    country_code TEXT NOT NULL,
    requested_analyses_json TEXT NOT NULL,
    objectives_json TEXT NOT NULL,
    service_targets_json TEXT NOT NULL,
    parameters_json TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS mapping_candidates (
    candidate_id TEXT PRIMARY KEY,
    component_id TEXT NOT NULL REFERENCES case_components(component_id) ON DELETE CASCADE,
    source_id TEXT NOT NULL REFERENCES case_sources(source_id) ON DELETE CASCADE,
    source_role TEXT NOT NULL,
    target_entity TEXT NOT NULL,
    target_field TEXT NOT NULL,
    source_field TEXT NOT NULL,
    transform_json TEXT NOT NULL,
    score REAL NOT NULL,
    reason_code TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS mapping_selections (
    component_id TEXT NOT NULL REFERENCES case_components(component_id) ON DELETE CASCADE,
    candidate_id TEXT NOT NULL REFERENCES mapping_candidates(candidate_id) ON DELETE RESTRICT,
    PRIMARY KEY(component_id, candidate_id)
);

CREATE TABLE IF NOT EXISTS demand_cities (
    component_id TEXT NOT NULL REFERENCES case_components(component_id) ON DELETE CASCADE,
    city_id TEXT NOT NULL,
    city_name TEXT NOT NULL,
    province_id TEXT,
    province_name TEXT,
    demand_quantity TEXT NOT NULL,
    longitude REAL,
    latitude REAL,
    PRIMARY KEY(component_id, city_id)
);

CREATE TABLE IF NOT EXISTS warehouses (
    component_id TEXT NOT NULL REFERENCES case_components(component_id) ON DELETE CASCADE,
    warehouse_id TEXT NOT NULL,
    warehouse_name TEXT NOT NULL,
    warehouse_type TEXT NOT NULL CHECK (warehouse_type IN ('center', 'cross_docking')),
    city_id TEXT NOT NULL,
    city_name TEXT NOT NULL,
    longitude REAL,
    latitude REAL,
    upstream_center_id TEXT,
    is_existing INTEGER NOT NULL CHECK (is_existing IN (0, 1)),
    is_fixed INTEGER NOT NULL CHECK (is_fixed IN (0, 1)),
    PRIMARY KEY(component_id, warehouse_id)
);

CREATE TABLE IF NOT EXISTS current_assignments (
    component_id TEXT NOT NULL REFERENCES case_components(component_id) ON DELETE CASCADE,
    demand_city_id TEXT NOT NULL,
    serving_warehouse_id TEXT NOT NULL,
    upstream_center_id TEXT,
    PRIMARY KEY(component_id, demand_city_id)
);

CREATE TABLE IF NOT EXISTS route_quotes (
    component_id TEXT NOT NULL REFERENCES case_components(component_id) ON DELETE CASCADE,
    origin_id TEXT NOT NULL,
    destination_id TEXT NOT NULL,
    layer TEXT NOT NULL CHECK (layer IN ('linehaul', 'last_mile')),
    price_per_vehicle TEXT NOT NULL,
    currency TEXT NOT NULL,
    vehicle_capacity TEXT NOT NULL,
    PRIMARY KEY(component_id, origin_id, destination_id, layer)
);

CREATE TABLE IF NOT EXISTS route_matrix_rows (
    component_id TEXT NOT NULL REFERENCES case_components(component_id) ON DELETE CASCADE,
    origin_id TEXT NOT NULL,
    destination_id TEXT NOT NULL,
    distance_km REAL NOT NULL CHECK (distance_km >= 0),
    duration_hours REAL NOT NULL CHECK (duration_hours >= 0),
    method TEXT NOT NULL,
    PRIMARY KEY(component_id, origin_id, destination_id)
);

CREATE TABLE IF NOT EXISTS cost_matrix_rows (
    component_id TEXT NOT NULL REFERENCES case_components(component_id) ON DELETE CASCADE,
    origin_id TEXT NOT NULL,
    destination_id TEXT NOT NULL,
    layer TEXT NOT NULL CHECK (layer IN ('linehaul', 'last_mile')),
    cost_per_demand_unit TEXT NOT NULL,
    currency TEXT NOT NULL,
    source TEXT NOT NULL CHECK (source IN ('quote', 'calculated')),
    PRIMARY KEY(component_id, origin_id, destination_id, layer)
);

CREATE TABLE IF NOT EXISTS assignment_rows (
    component_id TEXT NOT NULL REFERENCES case_components(component_id) ON DELETE CASCADE,
    demand_city_id TEXT NOT NULL,
    warehouse_id TEXT,
    upstream_center_id TEXT,
    demand_quantity TEXT NOT NULL,
    distance_km REAL,
    duration_hours REAL,
    cost TEXT,
    reason TEXT,
    PRIMARY KEY(component_id, demand_city_id)
);

CREATE TABLE IF NOT EXISTS service_metrics (
    component_id TEXT NOT NULL REFERENCES case_components(component_id) ON DELETE CASCADE,
    scope_id TEXT NOT NULL,
    target_hours REAL NOT NULL CHECK (target_hours > 0),
    covered_demand TEXT NOT NULL,
    total_demand TEXT NOT NULL,
    coverage_rate REAL NOT NULL CHECK (coverage_rate >= 0 AND coverage_rate <= 1),
    PRIMARY KEY(component_id, scope_id, target_hours)
);

CREATE TABLE IF NOT EXISTS warehouse_cost_summaries (
    component_id TEXT NOT NULL REFERENCES case_components(component_id) ON DELETE CASCADE,
    warehouse_id TEXT NOT NULL,
    linehaul_cost TEXT NOT NULL,
    last_mile_cost TEXT NOT NULL,
    total_cost TEXT NOT NULL,
    currency TEXT NOT NULL,
    PRIMARY KEY(component_id, warehouse_id)
);

CREATE TABLE IF NOT EXISTS network_scenarios (
    component_id TEXT PRIMARY KEY REFERENCES case_components(component_id) ON DELETE CASCADE,
    scenario_spec_json TEXT NOT NULL,
    result_summary_json TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS facility_location_solutions (
    component_id TEXT PRIMARY KEY REFERENCES case_components(component_id) ON DELETE CASCADE,
    solver_state TEXT NOT NULL,
    optimal INTEGER NOT NULL CHECK (optimal IN (0, 1)),
    feasible INTEGER NOT NULL CHECK (feasible IN (0, 1)),
    objective_value REAL,
    best_bound REAL,
    selected_warehouses_json TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS case_deliverables (
    deliverable_id TEXT PRIMARY KEY,
    case_id TEXT NOT NULL REFERENCES network_cases(case_id) ON DELETE CASCADE,
    component_id TEXT NOT NULL REFERENCES case_components(component_id) ON DELETE RESTRICT,
    artifact_schema TEXT NOT NULL,
    media_type TEXT NOT NULL,
    byte_size INTEGER NOT NULL CHECK (byte_size >= 0),
    content_sha256 TEXT NOT NULL CHECK (length(content_sha256) = 64),
    storage_name TEXT NOT NULL,
    created_at TEXT NOT NULL,
    UNIQUE(case_id, component_id, storage_name)
);

CREATE INDEX IF NOT EXISTS idx_case_components_case_kind
    ON case_components(case_id, component_kind, component_revision DESC);
CREATE INDEX IF NOT EXISTS idx_case_operations_running
    ON case_operations(case_id, state) WHERE state = 'running';
CREATE INDEX IF NOT EXISTS idx_network_cases_archived
    ON network_cases(archived_at) WHERE state = 'archived';

PRAGMA user_version = 1;

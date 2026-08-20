"""Owner module for route tools."""

from typing import (
    Annotated,
    Literal,
)

from mcp.server.fastmcp import (
    Context,
)
from mcp.types import (
    CallToolResult,
)
from open_web_codex_provider import (
    MAX_WORKSPACE_FILE_BYTES,
    ResourceRef,
)
from pydantic import (
    Field,
    ValidationError,
)
from supply_chain_planner.data.workspace_intake import (
    read_json_document,
)
from supply_chain_planner.network.matrix import (
    build_navigation_matrix_request,
    build_route_matrix_with_reuse,
)
from supply_chain_planner.network.matrix import (
    build_provided_route_matrix as _build_provided_route_matrix,
)
from supply_chain_planner.network.matrix import (
    register_navigation_route_matrix as _register_composable_navigation_matrix,
)
from supply_chain_planner.network.matrix_models import (
    NavigationMatrixResult,
)
from supply_chain_planner.network.matrix_models import (
    RouteMatrix as ComposableRouteMatrix,
)
from supply_chain_planner.shared.models import (
    NavigationMatrixRequestToolResult,
    RouteMatrixPreparationToolResult,
)
from supply_chain_planner.shared.planning_input import (
    require_matching_input,
)
from supply_chain_planner.shared.workspace_outputs import (
    WorkspaceOutputKind,
    prepare_workspace_output_path,
)

from .tool_runtime import (
    CONTENT_ADDRESSED_RESOURCE_TOOL,
    WORKSPACE_REQUEST_TOOL,
    McpResourceContractError,
    _load_ready_network,
    _runtime,
)


def create_navigation_matrix_request(
    prepared_input_relative_path: Annotated[str, Field(min_length=1, max_length=1024)],
    warehouse_scope: Literal["existing_only", "all_warehouses"],
    output_relative_path: Annotated[
        str,
        Field(
            min_length=1,
            max_length=1024,
            description=(
                "Create-new JSON path directly under outputs/warehouse-network/requests/."
            ),
        ),
    ],
    ctx: Context,
    prior_route_matrix_ref: ResourceRef | None = None,
) -> NavigationMatrixRequestToolResult:
    """Write the exact billable navigation lanes for one prepared Workspace input."""
    output_relative_path = prepare_workspace_output_path(
        _runtime().require_workspace(ctx),
        output_relative_path,
        WorkspaceOutputKind.NAVIGATION_REQUEST,
    )
    prepared, input_identity = _load_ready_network(prepared_input_relative_path, ctx)
    warehouses = prepared.warehouses
    if warehouse_scope == "existing_only":
        warehouses = [warehouse for warehouse in warehouses if warehouse.is_existing]
    request = build_navigation_matrix_request(
        prepared.demand_cities,
        warehouses,
        warehouse_scope=warehouse_scope,
        input_identity=input_identity,
    )
    reused_rows = []
    if prior_route_matrix_ref is not None:
        prior = _runtime().load_model(
            prior_route_matrix_ref,
            "route_matrix.v2",
            ComposableRouteMatrix,
        )
        try:
            require_matching_input(input_identity, prior.input_identity)
        except ValueError as error:
            raise McpResourceContractError("navigation_prior_input_identity_mismatch") from error
        if prior.warehouse_scope != warehouse_scope:
            raise McpResourceContractError("navigation_route_matrix_scope_mismatch")
        reused_rows = [
            row for row in prior.rows if row.method == "navigation" and row.status == "ready"
        ]
        reusable_keys = {(row.origin_id, row.destination_id, row.layer) for row in reused_rows}
        request = request.model_copy(
            update={
                "routes": [
                    route
                    for route in request.routes
                    if (route.origin_id, route.destination_id, route.layer) not in reusable_keys
                ],
                "estimated_billable_elements": len(
                    [
                        route
                        for route in request.routes
                        if (route.origin_id, route.destination_id, route.layer) not in reusable_keys
                    ]
                ),
            }
        )
    if not request.routes:
        return NavigationMatrixRequestToolResult(
            summary="All required navigation lane facts are already available from the exact prior matrix.",
            state="ready",
            navigation_request_relative_path=None,
            input_identity=input_identity,
            warehouse_scope=warehouse_scope,
            route_count=0,
            estimated_billable_elements=0,
        )
    created = _runtime().create_workspace_model(
        ctx,
        output_relative_path,
        request,
        max_bytes=MAX_WORKSPACE_FILE_BYTES,
    )
    return NavigationMatrixRequestToolResult(
        summary=(
            f"Prepared {len(request.routes)} exact navigation lanes for {warehouse_scope}; "
            f"{len(reused_rows)} exact navigation facts reused, estimated billable route elements: "
            f"{request.estimated_billable_elements}."
        ),
        state="execution_required",
        navigation_request_relative_path=created.relative_path,
        input_identity=input_identity,
        warehouse_scope=warehouse_scope,
        route_count=len(request.routes),
        estimated_billable_elements=request.estimated_billable_elements,
    )


def prepare_route_matrix(
    prepared_input_relative_path: Annotated[str, Field(min_length=1, max_length=1024)],
    route_method: Annotated[
        Literal["haversine", "provided"],
        Field(
            description=(
                "Route method for the required route pairs. Haversine requires both "
                "detour_coefficient and average_speed_kph."
            )
        ),
    ],
    ctx: Context,
    warehouse_scope: Literal["existing_only", "all_warehouses"] = "all_warehouses",
    detour_coefficient: Annotated[
        float | None,
        Field(description=("Required when route_method is haversine; omit for provided routes.")),
    ] = None,
    average_speed_kph: Annotated[
        float | None,
        Field(description=("Required when route_method is haversine; omit for provided routes.")),
    ] = None,
    prior_route_matrix_ref: ResourceRef | None = None,
) -> Annotated[CallToolResult, RouteMatrixPreparationToolResult]:
    """Prepare one provided or explicitly assumed haversine route matrix."""
    prepared, input_identity = _load_ready_network(prepared_input_relative_path, ctx)
    warehouses = prepared.warehouses
    if warehouse_scope == "existing_only":
        warehouses = [warehouse for warehouse in warehouses if warehouse.is_existing]
    prior = (
        _runtime().load_model(
            prior_route_matrix_ref,
            "route_matrix.v2",
            ComposableRouteMatrix,
        )
        if prior_route_matrix_ref is not None
        else None
    )
    if route_method == "provided":
        matrix = _build_provided_route_matrix(
            prepared.demand_cities,
            warehouses,
            prepared.provided_route_facts,
            warehouse_scope=warehouse_scope,
            input_identity=input_identity,
        )
    else:
        if detour_coefficient is None or average_speed_kph is None:
            raise McpResourceContractError("haversine_route_parameters_required")
        matrix = build_route_matrix_with_reuse(
            prepared.demand_cities,
            warehouses,
            prior.rows if prior is not None else [],
            detour_coefficient,
            average_speed_kph,
            warehouse_scope=warehouse_scope,
            input_identity=input_identity,
        )
    stats = matrix.stats
    result = _runtime().publish(
        matrix.schema_version,
        matrix,
        f"Prepared {route_method} route matrix for {warehouse_scope}; "
        f"{getattr(stats, 'reused_pair_count', 0)} reused, "
        f"{getattr(stats, 'computed_pair_count', getattr(stats, 'provided_pair_count', 0))} "
        f"materialized, and {stats.missing_pair_count} missing pairs.",
    )
    if result.structuredContent is None:
        raise McpResourceContractError("route_matrix_result_missing")
    result.structuredContent["state"] = "ready"
    return result


def import_navigation_matrix(
    prepared_input_relative_path: Annotated[str, Field(min_length=1, max_length=1024)],
    navigation_matrix_relative_path: Annotated[
        str,
        Field(min_length=1, max_length=1024),
    ],
    ctx: Context,
    prior_route_matrix_ref: ResourceRef | None = None,
) -> CallToolResult:
    """Validate and publish provider-executed navigation facts from the Workspace."""
    workspace = _runtime().require_workspace(ctx)
    prepared, input_identity = _load_ready_network(prepared_input_relative_path, ctx)
    document = read_json_document(workspace, navigation_matrix_relative_path)
    try:
        supplied = NavigationMatrixResult.model_validate(document)
    except ValidationError as error:
        raise McpResourceContractError("navigation_matrix_result_invalid") from error
    try:
        require_matching_input(input_identity, supplied.input_identity)
    except ValueError as error:
        raise McpResourceContractError("navigation_matrix_input_identity_mismatch") from error
    prior = (
        _runtime().load_model(
            prior_route_matrix_ref,
            "route_matrix.v2",
            ComposableRouteMatrix,
        )
        if prior_route_matrix_ref is not None
        else None
    )
    if prior is not None:
        try:
            require_matching_input(input_identity, prior.input_identity)
        except ValueError as error:
            raise McpResourceContractError("navigation_prior_input_identity_mismatch") from error
        if prior.warehouse_scope != supplied.warehouse_scope:
            raise McpResourceContractError("navigation_route_matrix_scope_mismatch")
    warehouses = prepared.warehouses
    if supplied.warehouse_scope == "existing_only":
        warehouses = [warehouse for warehouse in warehouses if warehouse.is_existing]
    prior_rows = prior.rows if prior is not None else []
    matrix = _register_composable_navigation_matrix(
        prepared.demand_cities,
        warehouses,
        [*prior_rows, *supplied.rows],
        warehouse_scope=supplied.warehouse_scope,
        input_identity=input_identity,
    )
    if matrix.missing_routes:
        raise McpResourceContractError("navigation_matrix_incomplete")
    matrix = matrix.model_copy(
        update={
            "stats": matrix.stats.model_copy(
                update={
                    "reused_pair_count": len(prior_rows),
                    "registered_pair_count": len(supplied.rows),
                    "missing_pair_count": 0,
                    "complete": True,
                }
            )
        }
    )
    return _runtime().publish(
        matrix.schema_version,
        matrix,
        f"Registered navigation matrix with {len(prior_rows)} reused and "
        f"{len(supplied.rows)} provider-executed pair facts.",
    )


def register_tools(mcp, *, phase: str = "all") -> None:
    if phase in ("all", "all"):
        mcp.tool(structured_output=True, annotations=WORKSPACE_REQUEST_TOOL)(create_navigation_matrix_request)
    if phase in ("all", "all"):
        mcp.tool(structured_output=True, annotations=CONTENT_ADDRESSED_RESOURCE_TOOL)(prepare_route_matrix)
    if phase in ("all", "all"):
        mcp.tool(structured_output=True, annotations=CONTENT_ADDRESSED_RESOURCE_TOOL)(import_navigation_matrix)

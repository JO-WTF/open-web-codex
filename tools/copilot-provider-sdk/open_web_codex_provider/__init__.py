"""Domain-neutral primitives for provider-owned MCP Resources."""

from .contracts import ResourceRef
from .errors import ProviderContractError, WorkspaceFileError
from .geojson import (
    GeoJsonFeatureTypeProfile,
    GeoJsonProfile,
    GeoJsonResourceRef,
    derive_geojson_profile,
)
from .runtime import McpResourceRuntime, bind_runtime
from .store import PublishedResource, ResourceStore, resource_ref, workspace_resource_root
from .workspace import (
    MAX_WORKSPACE_FILE_BYTES,
    CreatedWorkspaceFile,
    create_workspace_file,
    trusted_workspace_root,
)

__all__ = [
    "CreatedWorkspaceFile",
    "GeoJsonFeatureTypeProfile",
    "GeoJsonProfile",
    "GeoJsonResourceRef",
    "MAX_WORKSPACE_FILE_BYTES",
    "McpResourceRuntime",
    "ProviderContractError",
    "PublishedResource",
    "ResourceRef",
    "ResourceStore",
    "WorkspaceFileError",
    "bind_runtime",
    "create_workspace_file",
    "derive_geojson_profile",
    "resource_ref",
    "trusted_workspace_root",
    "workspace_resource_root",
]

__version__ = "0.1.0"

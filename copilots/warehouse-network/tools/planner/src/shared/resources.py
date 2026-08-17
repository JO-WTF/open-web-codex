"""One provider-owned Resource domain shared by supply-chain Tools.

Data and Network are separate MCP servers because they expose different tool
surfaces to different native Roles.  Their typed intermediate results belong
to the same supply-chain provider namespace, however.  This module makes that
ownership explicit without introducing a Platform resource registry.
"""

from __future__ import annotations

from pathlib import Path
from typing import TypeVar

from open_web_codex_provider import (
    McpResourceRuntime,
    ProviderContractError,
    ResourceRef,
    ResourceStore,
    bind_runtime,
    workspace_resource_root,
)
from pydantic import BaseModel
from supply_chain_planner.shared.resource_identity import (
    DATA_MCP_SERVER_NAME,
    NETWORK_MCP_SERVER_NAME,
    RESOURCE_PROVIDER_NAMESPACE,
)

RESOURCE_URI_PREFIX = "supply-chain://resources/"
ModelT = TypeVar("ModelT", bound=BaseModel)


class SupplyChainResources:
    """Typed access to one Profile + Workspace supply-chain Resource scope."""

    def __init__(self, workspace_root: Path, profile_state_root: Path) -> None:
        self.workspace_root = workspace_root.resolve(strict=True)
        self.profile_state_root = profile_state_root.resolve()
        self.store = ResourceStore(
            workspace_resource_root(
                self.profile_state_root,
                self.workspace_root,
                RESOURCE_PROVIDER_NAMESPACE,
            ),
            uri_prefix=RESOURCE_URI_PREFIX,
        )
        self.data = bind_runtime(
            self.workspace_root,
            self.profile_state_root,
            DATA_MCP_SERVER_NAME,
            RESOURCE_URI_PREFIX,
            store=self.store,
        )
        self.network = bind_runtime(
            self.workspace_root,
            self.profile_state_root,
            NETWORK_MCP_SERVER_NAME,
            RESOURCE_URI_PREFIX,
            store=self.store,
        )

    def runtime(self, server_name: str) -> McpResourceRuntime:
        if server_name == DATA_MCP_SERVER_NAME:
            return self.data
        if server_name == NETWORK_MCP_SERVER_NAME:
            return self.network
        raise ProviderContractError("resource_server_mismatch")

    def load_model(
        self,
        publisher_server: str,
        resource_ref: ResourceRef,
        expected_schema: str,
        model_type: type[ModelT],
    ) -> ModelT:
        """Load an exact Resource through its publishing supply-chain server."""

        return self.runtime(publisher_server).load_model(
            resource_ref,
            expected_schema,
            model_type,
        )

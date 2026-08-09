"""Shared MCP Resource runtime for one provider process."""

from __future__ import annotations

import json
from pathlib import Path
from typing import Any, TypeVar

from mcp.server.fastmcp import Context
from mcp.types import CallToolResult, ResourceLink, TextContent
from pydantic import BaseModel, ValidationError

from .mcp_contracts import ResourceRef
from .resource_store import ResourceStore, workspace_resource_root
from .workspace_files import CreatedWorkspaceFile, create_workspace_file
from .workspace_scope import trusted_workspace_root

MAX_RESOURCE_BYTES = 32 * 1024 * 1024
ModelT = TypeVar("ModelT", bound=BaseModel)


class McpResourceContractError(ValueError):
    """Stable failure for an invalid provider-owned Resource contract."""

    def __init__(self, code: str):
        self.code = code
        super().__init__(code)


class McpResourceRuntime:
    """Bind one provider process to its canonical Workspace Resource namespace."""

    def __init__(
        self,
        startup_workspace: Path,
        profile_home: Path,
        server_name: str,
        uri_prefix: str,
        *,
        store: ResourceStore | None = None,
    ) -> None:
        self.startup_workspace = startup_workspace.resolve(strict=True)
        self.server_name = server_name
        self.uri_prefix = uri_prefix
        if store is not None and store.uri_prefix != uri_prefix:
            raise ValueError("resource_store_uri_prefix_mismatch")
        self.store = store or ResourceStore(
            workspace_resource_root(
                profile_home,
                self.startup_workspace,
                server_name,
            ),
            uri_prefix=uri_prefix,
        )

    def require_workspace(self, ctx: Context) -> Path:
        try:
            workspace = trusted_workspace_root(ctx.request_context.meta)
            matches = workspace.samefile(self.startup_workspace)
        except (FileNotFoundError, OSError, ValueError) as error:
            raise McpResourceContractError("workspace_scope_invalid") from error
        if not matches:
            raise McpResourceContractError("workspace_scope_mismatch")
        return workspace

    def read(self, resource_id: str) -> str:
        try:
            return self.store.read(resource_id, max_bytes=MAX_RESOURCE_BYTES)
        except (OSError, ValueError) as error:
            raise McpResourceContractError("resource_read_invalid") from error

    def create_workspace_file(
        self,
        ctx: Context,
        relative_path: str,
        content: bytes,
        *,
        max_bytes: int,
    ) -> CreatedWorkspaceFile:
        workspace = self.require_workspace(ctx)
        return create_workspace_file(
            workspace,
            relative_path,
            content,
            max_bytes=max_bytes,
        )

    def create_workspace_model(
        self,
        ctx: Context,
        relative_path: str,
        value: BaseModel,
        *,
        max_bytes: int,
    ) -> CreatedWorkspaceFile:
        """Create one canonical JSON file from a validated typed model."""
        if not isinstance(value, BaseModel):
            raise McpResourceContractError("workspace_model_invalid")
        try:
            content = json.dumps(
                value.model_dump(mode="json", by_alias=True),
                ensure_ascii=False,
                sort_keys=True,
                separators=(",", ":"),
            ).encode("utf-8")
        except (TypeError, ValueError) as error:
            raise McpResourceContractError("workspace_model_invalid") from error
        try:
            return self.create_workspace_file(
                ctx,
                relative_path,
                content,
                max_bytes=max_bytes,
            )
        except McpResourceContractError:
            raise
        except (OSError, ValueError) as error:
            raise McpResourceContractError("workspace_file_invalid") from error

    def load_payload(self, ref: ResourceRef, expected_schema: str) -> dict[str, Any]:
        if ref.server != self.server_name:
            raise McpResourceContractError("resource_server_mismatch")
        if not ref.uri.startswith(self.uri_prefix):
            raise McpResourceContractError("resource_uri_mismatch")
        if ref.resource_schema != expected_schema:
            raise McpResourceContractError("resource_schema_mismatch")
        try:
            payload = self.store.load(ref, max_bytes=MAX_RESOURCE_BYTES)
        except (OSError, ValueError) as error:
            raise McpResourceContractError("resource_load_invalid") from error
        payload_schema = payload.get("schemaVersion", payload.get("schema_version"))
        if payload_schema != expected_schema:
            raise McpResourceContractError("resource_payload_schema_mismatch")
        return payload

    def load_model(
        self,
        ref: ResourceRef,
        expected_schema: str,
        model_type: type[ModelT],
    ) -> ModelT:
        payload = self.load_payload(ref, expected_schema)
        try:
            return model_type.model_validate(payload)
        except ValidationError as error:
            raise McpResourceContractError("resource_payload_invalid") from error

    def publish(
        self,
        schema: str,
        value: BaseModel | dict[str, Any],
        description: str,
    ) -> CallToolResult:
        payload_schema = (
            value.get("schemaVersion", value.get("schema_version"))
            if isinstance(value, dict)
            else getattr(value, "schema_version", None)
        )
        if payload_schema != schema:
            raise McpResourceContractError("resource_payload_schema_mismatch")
        try:
            published = self.store.publish(schema, value, max_bytes=MAX_RESOURCE_BYTES)
        except (OSError, TypeError, ValueError) as error:
            raise McpResourceContractError("resource_publish_invalid") from error
        ref = ResourceRef(
            server=self.server_name,
            uri=published.uri,
            resource_schema=published.schema,
        )
        return CallToolResult(
            content=[
                TextContent(type="text", text=description),
                ResourceLink(
                    type="resource_link",
                    name=published.resource_id,
                    title=published.schema,
                    uri=published.uri,
                    description=description,
                    mimeType="application/json",
                    size=published.size,
                ),
            ],
            structuredContent={
                "summary": description,
                "resource_ref": ref.model_dump(mode="json"),
            },
        )


def bind_runtime(
    startup_workspace: Path,
    profile_home: Path,
    server_name: str,
    uri_prefix: str,
    *,
    store: ResourceStore | None = None,
) -> McpResourceRuntime:
    return McpResourceRuntime(
        startup_workspace,
        profile_home,
        server_name,
        uri_prefix,
        store=store,
    )

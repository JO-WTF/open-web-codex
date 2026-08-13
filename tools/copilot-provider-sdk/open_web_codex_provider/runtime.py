"""Typed MCP Resource runtime for one provider process and Workspace scope."""

from __future__ import annotations

from collections.abc import Mapping
from pathlib import Path
from typing import Any, TypeVar

from mcp.server.fastmcp import Context
from mcp.types import CallToolResult, ResourceLink, TextContent
from pydantic import BaseModel, ValidationError

from .codec import MAX_RESOURCE_BYTES, canonical_json_bytes, require_payload_schema
from .contracts import ResourceRef
from .errors import ProviderContractError, WorkspaceFileError
from .store import ResourceStore, resource_ref, workspace_resource_root
from .workspace import CreatedWorkspaceFile, create_workspace_file, trusted_workspace_root

ModelT = TypeVar("ModelT", bound=BaseModel)


class McpResourceRuntime:
    def __init__(
        self,
        startup_workspace: Path,
        profile_home: Path,
        server_name: str,
        uri_prefix: str,
        *,
        store: ResourceStore | None = None,
    ) -> None:
        try:
            self.startup_workspace = startup_workspace.resolve(strict=True)
        except (OSError, RuntimeError) as error:
            raise ProviderContractError("workspace_scope_invalid") from error
        self.server_name = ResourceRef(
            server=server_name,
            uri="provider://validation/resource",
            resource_schema="provider.validation.v1",
        ).server
        self.uri_prefix = uri_prefix
        if store is not None and store.uri_prefix != uri_prefix:
            raise ProviderContractError("resource_store_uri_prefix_mismatch")
        self.store = store or ResourceStore(
            workspace_resource_root(profile_home, self.startup_workspace, server_name),
            uri_prefix=uri_prefix,
        )

    def require_workspace(self, ctx: Context) -> Path:
        try:
            workspace = trusted_workspace_root(ctx.request_context.meta)
            matches = workspace.samefile(self.startup_workspace)
        except (OSError, WorkspaceFileError) as error:
            raise ProviderContractError("workspace_scope_invalid") from error
        if not matches:
            raise ProviderContractError("workspace_scope_mismatch")
        return workspace

    def read(self, resource_id: str) -> str:
        try:
            return self.store.read(resource_id, max_bytes=MAX_RESOURCE_BYTES)
        except ProviderContractError as error:
            raise ProviderContractError("resource_read_invalid") from error

    def create_workspace_file(
        self,
        ctx: Context,
        relative_path: str,
        content: bytes,
        *,
        max_bytes: int,
    ) -> CreatedWorkspaceFile:
        try:
            return create_workspace_file(
                self.require_workspace(ctx), relative_path, content, max_bytes=max_bytes
            )
        except ProviderContractError:
            raise
        except WorkspaceFileError as error:
            raise ProviderContractError("workspace_file_invalid") from error

    def create_workspace_model(
        self,
        ctx: Context,
        relative_path: str,
        value: BaseModel,
        *,
        max_bytes: int,
    ) -> CreatedWorkspaceFile:
        try:
            content = canonical_json_bytes(value, max_bytes=max_bytes)
            return self.create_workspace_file(
                ctx, relative_path, content, max_bytes=max_bytes
            )
        except ProviderContractError as error:
            if error.code.startswith("workspace_"):
                raise
            raise ProviderContractError("workspace_file_invalid") from error

    def load_payload(self, ref: ResourceRef, expected_schema: str) -> dict[str, Any]:
        if ref.server != self.server_name:
            raise ProviderContractError("resource_server_mismatch")
        if not ref.uri.startswith(self.uri_prefix):
            raise ProviderContractError("resource_uri_mismatch")
        if ref.resource_schema != expected_schema:
            raise ProviderContractError("resource_schema_mismatch")
        try:
            payload = self.store.load(ref, max_bytes=MAX_RESOURCE_BYTES)
        except ProviderContractError as error:
            raise ProviderContractError("resource_load_invalid") from error
        require_payload_schema(payload, expected_schema)
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
            raise ProviderContractError("resource_payload_invalid") from error

    def publish(
        self,
        schema: str,
        value: BaseModel | dict[str, Any],
        description: str,
        *,
        mime_type: str = "application/json",
        reference_fields: Mapping[str, BaseModel] | None = None,
    ) -> CallToolResult:
        payload = (
            value.model_dump(mode="json", by_alias=True)
            if isinstance(value, BaseModel)
            else value
        )
        require_payload_schema(payload, schema)
        try:
            published = self.store.publish(schema, value, max_bytes=MAX_RESOURCE_BYTES)
        except ProviderContractError as error:
            raise ProviderContractError("resource_publish_invalid") from error
        ref = resource_ref(published, self.server_name)
        structured_content: dict[str, Any] = {
            "summary": description,
            "resource_ref": ref.model_dump(mode="json"),
        }
        for name, reference in (reference_fields or {}).items():
            if not isinstance(name, str) or not name or len(name) > 128:
                raise ProviderContractError("resource_reference_field_invalid")
            if not isinstance(reference, BaseModel):
                raise ProviderContractError("resource_reference_field_invalid")
            structured_content[name] = reference.model_dump(mode="json", by_alias=True)
        return CallToolResult(
            content=[
                TextContent(type="text", text=description),
                ResourceLink(
                    type="resource_link",
                    name=published.resource_id,
                    title=published.schema,
                    uri=published.uri,
                    description=description,
                    mimeType=mime_type,
                    size=published.size,
                ),
            ],
            structuredContent=structured_content,
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

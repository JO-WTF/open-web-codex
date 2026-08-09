"""Domain-neutral contracts shared by MCP Resource providers."""

from __future__ import annotations

from typing import Literal

from pydantic import BaseModel, ConfigDict, Field


class ResourceRef(BaseModel):
    """An exact provider-owned MCP Resource reference."""

    model_config = ConfigDict(extra="forbid")

    type: Literal["mcp_resource"] = "mcp_resource"
    server: str = Field(min_length=1, max_length=128, pattern=r"^[a-z][a-z0-9_.-]*$")
    uri: str = Field(min_length=1, max_length=2048)
    resource_schema: str = Field(
        min_length=1,
        max_length=128,
        pattern=r"^[a-z][a-z0-9_.-]*$",
    )


class MapResourceRef(BaseModel):
    """A reviewed local GeoJSON Resource accepted by map-card authoring."""

    model_config = ConfigDict(extra="forbid")

    type: Literal["mcp_resource"] = "mcp_resource"
    server: str = Field(min_length=1, max_length=128, pattern=r"^[a-z][a-z0-9_.-]*$")
    uri: str = Field(min_length=1, max_length=2048)
    format: Literal["geojson"] = "geojson"

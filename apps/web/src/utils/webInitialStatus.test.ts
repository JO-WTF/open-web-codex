import { describe, expect, it } from "vitest";
import { formatMcpStatusLines, mergeRateLimits, parseInitialMcpServers, parseInitialRateLimits } from "./webInitialStatus";

describe("initial workspace status parsing", () => {
  it("maps the MCP inventory snapshot to sidebar server states", () => {
    expect(parseInitialMcpServers({
      result: {
        data: [
          { name: "docs", serverInfo: { name: "docs" }, tools: {}, resources: [], resourceTemplates: [], authStatus: "unsupported" },
          { name: "github", serverInfo: null, tools: {}, resources: [], resourceTemplates: [], authStatus: "notLoggedIn" },
        ],
      },
    })).toEqual({
      docs: { name: "docs", status: "ready", failureReason: null },
      github: { name: "github", status: "unavailable", failureReason: "Authentication required" },
    });
  });

  it("formats MCP inventory as a concise Chinese list", () => {
    expect(formatMcpStatusLines({
      result: {
        data: [
          {
            name: "maps",
            status: "ready",
            tools: { mcp__maps__get_route: {}, mcp__maps__batch_geocode: {} },
          },
        ],
      },
    })).toEqual([
      "可用 MCP：",
      "- maps（ready）",
      "  工具：batch_geocode、get_route",
    ]);
  });

  it("formats an empty MCP inventory without local path details", () => {
    expect(formatMcpStatusLines({ result: { data: [] } })).toEqual([
      "可用 MCP：",
      "- 暂无已配置的 MCP 服务。",
    ]);
  });

  it("extracts the canonical rate-limit snapshot", () => {
    const rateLimits = {
      primary: { usedPercent: 12 },
      secondary: { usedPercent: 34 },
    };
    expect(parseInitialRateLimits({ result: { rateLimits } })).toEqual(rateLimits);
  });

  it("merges sparse usage events without discarding snapshot fields", () => {
    expect(mergeRateLimits(
      { primary: { usedPercent: 12, resetsAt: 100 }, planType: "plus" },
      { primary: { usedPercent: 18 } },
    )).toEqual({
      primary: { usedPercent: 18, resetsAt: 100 },
      planType: "plus",
    });
  });
});

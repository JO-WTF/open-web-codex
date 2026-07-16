import { describe, expect, it } from "vitest";
import {
  appendRunEvents,
  readStoredActiveProjectId,
  readStoredActiveTaskId,
  resolveActiveTaskId,
  runStartIdempotencyKey,
  shouldPollTaskRun,
  writeStoredActiveProjectId,
  writeStoredActiveTaskId,
} from "./platformWebAppHelpers";

describe("platformWebAppHelpers", () => {
  it("dedupes run events by sequence", () => {
    const existing = [{ sequence: 1, id: "a" } as never];
    const incoming = [
      { sequence: 1, id: "a-dup" } as never,
      { sequence: 2, id: "b" } as never,
    ];
    expect(appendRunEvents(existing, incoming)).toEqual([
      { sequence: 1, id: "a" },
      { sequence: 2, id: "b" },
    ]);
  });

  it("uses stable idempotency keys per task", () => {
    expect(runStartIdempotencyKey("task-1")).toBe("active-run:task-1");
  });

  it("polls only while a run can still change", () => {
    expect(shouldPollTaskRun("running")).toBe(true);
    expect(shouldPollTaskRun("waiting_approval")).toBe(true);
    expect(shouldPollTaskRun("completed")).toBe(false);
  });

  it("restores stored project and task selections", () => {
    localStorage.clear();
    writeStoredActiveProjectId("project-b");
    writeStoredActiveTaskId("project-b", "task-2");
    expect(readStoredActiveProjectId()).toBe("project-b");
    expect(readStoredActiveTaskId("project-b")).toBe("task-2");
    expect(resolveActiveTaskId(
      [{ id: "task-1" }, { id: "task-2" }],
      readStoredActiveTaskId("project-b"),
    )).toBe("task-2");
    expect(resolveActiveTaskId(
      [{ id: "task-1" }],
      readStoredActiveTaskId("project-b"),
    )).toBe("task-1");
    localStorage.clear();
  });
});

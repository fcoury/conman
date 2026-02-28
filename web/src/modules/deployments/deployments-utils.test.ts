import { describe, expect, it } from "vitest";

import {
  classifyDeploymentState,
  countDeploymentHistoryByState,
  filterDeploymentHistory,
} from "@/modules/deployments/deployments-utils";
import type { Deployment } from "@/types/api";

function dep(id: string, state: string, env = "e1", release = "rel1"): Deployment {
  return {
    id,
    repo_id: "r1",
    environment_id: env,
    release_id: release,
    state,
    is_skip_stage: false,
    is_concurrent_batch: false,
    approvals: [],
    created_by: "u1",
    created_at: `2026-01-01T00:00:0${id.length}Z`,
    updated_at: `2026-01-01T00:00:0${id.length}Z`,
  };
}

describe("deployment utils", () => {
  it("classifies state values", () => {
    expect(classifyDeploymentState("running")).toBe("running");
    expect(classifyDeploymentState("succeeded")).toBe("succeeded");
    expect(classifyDeploymentState("failed")).toBe("failed");
  });

  it("counts and filters deployment history", () => {
    const items = [dep("a", "running"), dep("bb", "succeeded", "e2"), dep("ccc", "failed")];
    const counts = countDeploymentHistoryByState(items);
    expect(counts.all).toBe(3);
    expect(counts.running).toBe(1);

    const filtered = filterDeploymentHistory(items, "e2", "succeeded", "rel1");
    expect(filtered).toHaveLength(1);
    expect(filtered[0].environment_id).toBe("e2");
  });
});

import { describe, expect, it } from "vitest";

import {
  countChangesetsByState,
  filterChangesetsByState,
  parseOverrides,
  stateCategory,
} from "@/modules/changesets/changesets-utils";
import type { Changeset } from "@/types/api";

function cs(id: string, state: string): Changeset {
  return {
    id,
    repo_id: "r1",
    workspace_id: "w1",
    title: id,
    state,
    author_user_id: "u1",
    head_sha: "abc",
    revision: 1,
    approvals: [],
    created_at: "2026-01-01T00:00:00Z",
    updated_at: "2026-01-01T00:00:00Z",
  };
}

describe("changesets utils", () => {
  it("categorizes state families", () => {
    expect(stateCategory("queued")).toBe("queued");
    expect(stateCategory("approved")).toBe("approved");
    expect(stateCategory("in_review")).toBe("review");
    expect(stateCategory("draft")).toBe("draft");
  });

  it("parses overrides array", () => {
    expect(parseOverrides("[]")).toEqual([]);
    expect(() => parseOverrides("{}")).toThrow("profile overrides must be a JSON array");
  });

  it("counts and filters by state", () => {
    const items = [cs("a", "draft"), cs("b", "approved"), cs("c", "queued")];
    expect(countChangesetsByState(items)).toEqual({
      all: 3,
      draft: 1,
      review: 0,
      approved: 1,
      queued: 1,
    });
    expect(filterChangesetsByState(items, "queued").map((x) => x.id)).toEqual(["c"]);
  });
});

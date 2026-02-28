import { describe, expect, it } from "vitest";

import { summarizeReleaseImpact } from "@/modules/releases/releases-impact-utils";
import type { Changeset } from "@/types/api";

function cs(id: string, state: string, author: string): Changeset {
  return {
    id,
    repo_id: "r1",
    workspace_id: "w1",
    title: id,
    state,
    author_user_id: author,
    head_sha: "abc",
    revision: 1,
    approvals: [],
    created_at: "2026-01-01T00:00:00Z",
    updated_at: "2026-01-01T00:00:00Z",
  };
}

describe("release impact utils", () => {
  it("summarizes selected changesets and diff payload paths", () => {
    const changesets = [cs("a", "queued", "u1"), cs("b", "queued", "u2")];
    const diffPayloads = [
      { files: [{ path: "config/app.yml" }, { file_path: "routes/home.yml" }] },
      { nested: { route: "checkout" } },
    ];

    const summary = summarizeReleaseImpact(changesets, diffPayloads);

    expect(summary.selectedCount).toBe(2);
    expect(summary.uniqueAuthors).toBe(2);
    expect(summary.changedPathCount).toBeGreaterThanOrEqual(3);
    expect(summary.changedPathsPreview).toContain("config/app.yml");
  });
});

import type { Changeset } from "@/types/api";

export type ChangesetFilterState = "all" | "draft" | "review" | "approved" | "queued";

export function stateCategory(state: string): Exclude<ChangesetFilterState, "all"> {
  const normalized = state.toLowerCase();
  if (normalized.includes("queue")) {
    return "queued";
  }
  if (normalized.includes("approve")) {
    return "approved";
  }
  if (normalized.includes("review")) {
    return "review";
  }
  return "draft";
}

export function parseOverrides(raw: string): unknown[] {
  const parsed = JSON.parse(raw);
  if (!Array.isArray(parsed)) {
    throw new Error("profile overrides must be a JSON array");
  }
  return parsed;
}

export function countChangesetsByState(changesets: Changeset[]): Record<ChangesetFilterState, number> {
  return {
    all: changesets.length,
    draft: changesets.filter((changeset) => stateCategory(changeset.state) === "draft").length,
    review: changesets.filter((changeset) => stateCategory(changeset.state) === "review").length,
    approved: changesets.filter((changeset) => stateCategory(changeset.state) === "approved").length,
    queued: changesets.filter((changeset) => stateCategory(changeset.state) === "queued").length,
  };
}

export function filterChangesetsByState(
  changesets: Changeset[],
  state: ChangesetFilterState,
): Changeset[] {
  if (state === "all") {
    return changesets;
  }
  return changesets.filter((changeset) => stateCategory(changeset.state) === state);
}

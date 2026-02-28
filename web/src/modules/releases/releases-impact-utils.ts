import type { Changeset } from "@/types/api";

export interface ReleaseImpactSummary {
  selectedCount: number;
  stateCounts: Record<string, number>;
  uniqueAuthors: number;
  changedPathCount: number;
  changedPathsPreview: string[];
}

function collectPathStrings(value: unknown, acc: Set<string>): void {
  if (value === null || value === undefined) {
    return;
  }

  if (Array.isArray(value)) {
    for (const item of value) {
      collectPathStrings(item, acc);
    }
    return;
  }

  if (typeof value === "object") {
    const record = value as Record<string, unknown>;
    for (const [key, nested] of Object.entries(record)) {
      if (typeof nested === "string") {
        const lowerKey = key.toLowerCase();
        const looksLikePathKey =
          lowerKey === "path" ||
          lowerKey.endsWith("_path") ||
          lowerKey.includes("file") ||
          lowerKey.includes("route");
        if (looksLikePathKey && nested.trim().length > 0) {
          acc.add(nested.trim());
        }
      }
      collectPathStrings(nested, acc);
    }
  }
}

export function summarizeReleaseImpact(
  selectedChangesets: Changeset[],
  diffPayloads: unknown[],
): ReleaseImpactSummary {
  const stateCounts: Record<string, number> = {};
  const authorIds = new Set<string>();

  for (const changeset of selectedChangesets) {
    const key = changeset.state || "unknown";
    stateCounts[key] = (stateCounts[key] ?? 0) + 1;
    if (changeset.author_user_id) {
      authorIds.add(changeset.author_user_id);
    }
  }

  const changedPathSet = new Set<string>();
  for (const payload of diffPayloads) {
    collectPathStrings(payload, changedPathSet);
  }

  const changedPathsPreview = [...changedPathSet].slice(0, 12);

  return {
    selectedCount: selectedChangesets.length,
    stateCounts,
    uniqueAuthors: authorIds.size,
    changedPathCount: changedPathSet.size,
    changedPathsPreview,
  };
}

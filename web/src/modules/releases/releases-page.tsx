import { useEffect, useMemo, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";

import { ApiError } from "@/api/client";
import { Button } from "@/components/ui/button";
import { Card, CardDescription, CardTitle } from "@/components/ui/card";
import { RawDataPanel } from "@/components/ui/raw-data-panel";
import { Select } from "@/components/ui/select";
import { StatusPill } from "@/components/ui/status-pill";
import { useApi } from "@/hooks/use-api";
import { useRepoContext } from "@/hooks/use-repo-context";
import { canManageReleases, formatRoleLabel } from "@/lib/rbac";
import { formatDate } from "@/lib/utils";
import { Page } from "@/modules/shared/page";
import type { Changeset, ReleaseBatch } from "@/types/api";

export function ReleasesPage(): React.ReactElement {
  const api = useApi();
  const queryClient = useQueryClient();
  const context = useRepoContext();
  const repoId = context?.repo?.id;
  const role = context?.role;

  const [selectedReleaseId, setSelectedReleaseId] = useState("");
  const [selectedChangesetIds, setSelectedChangesetIds] = useState<string[]>([]);
  const [status, setStatus] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const canManage = canManageReleases(role);

  const releasesQuery = useQuery({
    queryKey: ["releases", repoId],
    queryFn: async () => {
      const envelope = await api.paginated<ReleaseBatch[]>(`/api/repos/${repoId}/releases?page=1&limit=100`);
      return envelope.data;
    },
    enabled: Boolean(repoId),
  });

  const changesetsQuery = useQuery({
    queryKey: ["releases", "changesets", repoId],
    queryFn: async () => {
      const envelope = await api.paginated<Changeset[]>(`/api/repos/${repoId}/changesets?page=1&limit=200`);
      return envelope.data;
    },
    enabled: Boolean(repoId),
  });

  const queuedChangesets = useMemo(
    () => (changesetsQuery.data ?? []).filter((changeset) => changeset.state.toLowerCase().includes("queue")),
    [changesetsQuery.data],
  );

  useEffect(() => {
    if (!selectedReleaseId && releasesQuery.data?.[0]?.id) {
      setSelectedReleaseId(releasesQuery.data[0].id);
    }
  }, [selectedReleaseId, releasesQuery.data]);

  const selectedRelease = useMemo(
    () => releasesQuery.data?.find((release) => release.id === selectedReleaseId) ?? null,
    [releasesQuery.data, selectedReleaseId],
  );

  useEffect(() => {
    if (selectedRelease?.ordered_changeset_ids) {
      setSelectedChangesetIds(selectedRelease.ordered_changeset_ids);
    } else {
      setSelectedChangesetIds([]);
    }
  }, [selectedRelease]);

  const refresh = async (): Promise<void> => {
    await queryClient.invalidateQueries({ queryKey: ["releases", repoId] });
    await queryClient.invalidateQueries({ queryKey: ["releases", "changesets", repoId] });
    await queryClient.invalidateQueries({ queryKey: ["jobs", repoId] });
  };

  const perform = async (fn: () => Promise<void>, successMessage: string): Promise<void> => {
    setError(null);
    setStatus(null);
    try {
      await fn();
      await refresh();
      setStatus(successMessage);
    } catch (cause) {
      setError(cause instanceof ApiError ? cause.message : "release action failed");
    }
  };

  const createRelease = async (): Promise<void> => {
    if (!repoId || !canManage) return;
    await perform(
      async () => {
        const created = await api.data<ReleaseBatch>(`/api/repos/${repoId}/releases`, {
          method: "POST",
          body: JSON.stringify({}),
        });
        setSelectedReleaseId(created.id);
      },
      "Release draft created.",
    );
  };

  const toggleChangeset = (changesetId: string): void => {
    setSelectedChangesetIds((prev) =>
      prev.includes(changesetId) ? prev.filter((id) => id !== changesetId) : [...prev, changesetId],
    );
  };

  const moveChangeset = (index: number, direction: -1 | 1): void => {
    const nextIndex = index + direction;
    if (nextIndex < 0 || nextIndex >= selectedChangesetIds.length) {
      return;
    }
    setSelectedChangesetIds((prev) => {
      const next = [...prev];
      const [item] = next.splice(index, 1);
      next.splice(nextIndex, 0, item);
      return next;
    });
  };

  const selectedChangesets = useMemo(() => {
    const map = new Map((changesetsQuery.data ?? []).map((changeset) => [changeset.id, changeset]));
    return selectedChangesetIds
      .map((id) => map.get(id))
      .filter((changeset): changeset is Changeset => Boolean(changeset));
  }, [changesetsQuery.data, selectedChangesetIds]);

  if (!repoId) {
    return <Page title="Releases">Bind a repo first in Setup.</Page>;
  }

  return (
    <Page
      title="Releases"
      description="Build release batches from queued changesets, set merge order, assemble, and publish."
    >
      {error ? <Card className="border-destructive/40 bg-destructive/10 p-3 text-sm">{error}</Card> : null}
      {status ? (
        <Card className="border-success/40 bg-success/40 p-3 text-sm" aria-live="polite">
          {status}
        </Card>
      ) : null}

      <Card>
        <CardTitle>Role Scope</CardTitle>
        <CardDescription>
          You are signed in as {formatRoleLabel(role)}.
          {canManage
            ? " Create release drafts, compose queued changesets, then assemble and publish."
            : " You can view release state, but only Config Manager and above can change releases."}
        </CardDescription>
      </Card>

      <div className="grid gap-4 xl:grid-cols-[340px_1fr]">
        <Card className="space-y-3">
          <CardTitle>Release Batches</CardTitle>
          <Button type="button" onClick={() => void createRelease()} disabled={!canManage}>
            Create release draft
          </Button>

          <Select
            value={selectedReleaseId}
            onChange={(event) => setSelectedReleaseId(event.target.value)}
            aria-label="Select release"
          >
            <option value="">Select release</option>
            {(releasesQuery.data ?? []).map((release) => (
              <option key={release.id} value={release.id}>
                {release.tag || release.id} ({release.state})
              </option>
            ))}
          </Select>

          <div className="max-h-[420px] space-y-2 overflow-auto pr-1">
            {(releasesQuery.data ?? []).map((release) => (
              <button
                key={release.id}
                type="button"
                className={`w-full rounded-md border p-3 text-left transition-colors ${
                  selectedReleaseId === release.id
                    ? "border-primary bg-primary/10"
                    : "border-border bg-muted/30 hover:bg-accent"
                }`}
                onClick={() => setSelectedReleaseId(release.id)}
              >
                <div className="flex items-center justify-between gap-2">
                  <span className="text-sm font-semibold">{release.tag || release.id}</span>
                  <StatusPill label={release.state} />
                </div>
                <p className="mt-1 text-xs text-muted-foreground">updated {formatDate(release.updated_at)}</p>
              </button>
            ))}
            {!releasesQuery.data?.length ? <p className="text-sm text-muted-foreground">No releases yet.</p> : null}
          </div>
        </Card>

        <Card className="space-y-4">
          <CardTitle>Release Composer</CardTitle>
          {!selectedRelease ? (
            <p className="text-sm text-muted-foreground">Create or select a release to start composition.</p>
          ) : (
            <>
              <Card className="bg-muted/30 p-3">
                <div className="flex items-center gap-2">
                  <h3 className="text-sm font-semibold">{selectedRelease.tag || selectedRelease.id}</h3>
                  <StatusPill label={selectedRelease.state} />
                </div>
                <p className="mt-1 text-xs text-muted-foreground">Published SHA: {selectedRelease.published_sha || "-"}</p>
              </Card>

              <div className="grid gap-4 lg:grid-cols-2">
                <Card className="space-y-2 border border-border/60 p-3">
                  <CardTitle className="text-sm">Queued Changesets</CardTitle>
                  <CardDescription>Select queued changesets to include in this release.</CardDescription>
                  <div className="max-h-[300px] space-y-2 overflow-auto pr-1">
                    {queuedChangesets.map((changeset) => {
                      const checked = selectedChangesetIds.includes(changeset.id);
                      return (
                        <label
                          key={changeset.id}
                          className="flex cursor-pointer items-start gap-2 rounded-md border border-border bg-muted/20 p-2"
                        >
                          <input
                            type="checkbox"
                            checked={checked}
                            onChange={() => toggleChangeset(changeset.id)}
                            className="mt-0.5"
                            disabled={!canManage}
                          />
                          <span className="min-w-0">
                            <span className="block truncate text-sm font-medium">{changeset.title}</span>
                            <span className="text-xs text-muted-foreground">{changeset.id}</span>
                          </span>
                        </label>
                      );
                    })}
                    {!queuedChangesets.length ? (
                      <p className="text-sm text-muted-foreground">No queued changesets available.</p>
                    ) : null}
                  </div>
                </Card>

                <Card className="space-y-2 border border-border/60 p-3">
                  <CardTitle className="text-sm">Composition Order</CardTitle>
                  <CardDescription>Order controls merge sequence during assemble.</CardDescription>
                  <div className="max-h-[300px] space-y-2 overflow-auto pr-1">
                    {selectedChangesets.map((changeset, index) => (
                      <div key={changeset.id} className="rounded-md border border-border bg-muted/20 p-2">
                        <div className="flex items-start justify-between gap-2">
                          <div className="min-w-0">
                            <p className="truncate text-sm font-medium">{changeset.title}</p>
                            <p className="text-xs text-muted-foreground">{changeset.id}</p>
                          </div>
                          <div className="flex gap-1">
                            <Button
                              type="button"
                              variant="outline"
                              size="sm"
                              onClick={() => moveChangeset(index, -1)}
                              disabled={!canManage || index === 0}
                            >
                              Up
                            </Button>
                            <Button
                              type="button"
                              variant="outline"
                              size="sm"
                              onClick={() => moveChangeset(index, 1)}
                              disabled={!canManage || index === selectedChangesets.length - 1}
                            >
                              Down
                            </Button>
                          </div>
                        </div>
                      </div>
                    ))}
                    {!selectedChangesets.length ? (
                      <p className="text-sm text-muted-foreground">No changesets selected yet.</p>
                    ) : null}
                  </div>
                </Card>
              </div>

              <div className="flex flex-wrap gap-2">
                <Button
                  type="button"
                  variant="secondary"
                  disabled={!selectedReleaseId || !canManage}
                  onClick={() =>
                    void perform(
                      async () => {
                        await api.data(`/api/repos/${repoId}/releases/${selectedReleaseId}/changesets`, {
                          method: "POST",
                          body: JSON.stringify({ changeset_ids: selectedChangesetIds }),
                        });
                      },
                      "Release changesets updated.",
                    )
                  }
                >
                  Save selection
                </Button>
                <Button
                  type="button"
                  variant="secondary"
                  disabled={!selectedReleaseId || !canManage}
                  onClick={() =>
                    void perform(
                      async () => {
                        await api.data(`/api/repos/${repoId}/releases/${selectedReleaseId}/reorder`, {
                          method: "POST",
                          body: JSON.stringify({ changeset_ids: selectedChangesetIds }),
                        });
                      },
                      "Release order updated.",
                    )
                  }
                >
                  Save order
                </Button>
                <Button
                  type="button"
                  variant="secondary"
                  disabled={!selectedReleaseId || !canManage}
                  onClick={() =>
                    void perform(
                      async () => {
                        await api.data(`/api/repos/${repoId}/releases/${selectedReleaseId}/assemble`, {
                          method: "POST",
                          body: JSON.stringify({}),
                        });
                      },
                      "Release assembly started.",
                    )
                  }
                >
                  Assemble
                </Button>
                <Button
                  type="button"
                  disabled={!selectedReleaseId || !canManage}
                  onClick={() =>
                    void perform(
                      async () => {
                        await api.data(`/api/repos/${repoId}/releases/${selectedReleaseId}/publish`, {
                          method: "POST",
                          body: JSON.stringify({}),
                        });
                      },
                      "Release published.",
                    )
                  }
                >
                  Publish
                </Button>
              </div>
            </>
          )}
        </Card>
      </div>

      <details>
        <summary className="cursor-pointer text-xs text-muted-foreground">Advanced release payload</summary>
        <div className="mt-2">
          <RawDataPanel title="Releases payload" value={releasesQuery.data ?? []} />
        </div>
      </details>
    </Page>
  );
}

import { FormEvent, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";

import { ApiError } from "@/api/client";
import { Button } from "@/components/ui/button";
import { Card, CardDescription, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { RawDataPanel } from "@/components/ui/raw-data-panel";
import { Textarea } from "@/components/ui/textarea";
import { useApi } from "@/hooks/use-api";
import { useRepoContext } from "@/hooks/use-repo-context";
import { canManageReleases, formatRoleLabel } from "@/lib/rbac";
import { Page } from "@/modules/shared/page";
import type { ReleaseBatch } from "@/types/api";

export function ReleasesPage(): React.ReactElement {
  const api = useApi();
  const queryClient = useQueryClient();
  const context = useRepoContext();
  const repoId = context?.repo?.id;
  const role = context?.role;

  const [selectedReleaseId, setSelectedReleaseId] = useState("");
  const [changesetIdsCsv, setChangesetIdsCsv] = useState("");
  const [reorderBody, setReorderBody] = useState("[]");
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

  const refresh = async (): Promise<void> => {
    await queryClient.invalidateQueries({ queryKey: ["releases", repoId] });
    await queryClient.invalidateQueries({ queryKey: ["jobs", repoId] });
  };

  const perform = async (fn: () => Promise<void>): Promise<void> => {
    setError(null);
    try {
      await fn();
      await refresh();
    } catch (cause) {
      setError(cause instanceof ApiError ? cause.message : "release action failed");
    }
  };

  const createRelease = async (event: FormEvent<HTMLFormElement>): Promise<void> => {
    event.preventDefault();
    if (!repoId || !canManage) return;
    await perform(async () => {
      const created = await api.data<ReleaseBatch>(`/api/repos/${repoId}/releases`, {
        method: "POST",
        body: JSON.stringify({}),
      });
      setSelectedReleaseId(created.id);
    });
  };

  if (!repoId) {
    return <Page title="Releases">Bind a repo first in Setup.</Page>;
  }

  return (
    <Page
      title="Releases"
      description="Config managers and above assemble approved changesets into release batches and publish them."
    >
      {error ? <Card className="border-destructive/40 bg-destructive/10 p-3 text-sm">{error}</Card> : null}

      <Card>
        <CardTitle>Role Scope</CardTitle>
        <CardDescription>
          You are signed in as {formatRoleLabel(role)}.
          {canManage
            ? " Build release batches, set order, assemble, and publish to deployment flow."
            : " You can view release state, but only Config Manager and above can change releases."}
        </CardDescription>
      </Card>

      <Card className="space-y-2">
        <CardTitle>Create Release Draft</CardTitle>
        <form onSubmit={(event) => void createRelease(event)}>
          <Button type="submit" disabled={!canManage}>
            Create release draft
          </Button>
        </form>
      </Card>

      <Card className="space-y-3">
        <CardTitle>Release Actions</CardTitle>
        <CardDescription>1) Attach changesets, 2) reorder if needed, 3) assemble, 4) publish.</CardDescription>
        <Input
          value={selectedReleaseId}
          onChange={(event) => setSelectedReleaseId(event.target.value)}
          placeholder="release id"
        />
        <Input
          value={changesetIdsCsv}
          onChange={(event) => setChangesetIdsCsv(event.target.value)}
          placeholder="changeset ids csv"
          disabled={!canManage}
        />
        <details>
          <summary className="cursor-pointer text-xs text-muted-foreground">Advanced reorder payload</summary>
          <Textarea
            value={reorderBody}
            onChange={(event) => setReorderBody(event.target.value)}
            className="mt-2 min-h-28 font-mono"
            placeholder='["changeset-id-1","changeset-id-2"]'
            disabled={!canManage}
          />
        </details>
        <div className="flex flex-wrap gap-2">
          <Button
            type="button"
            variant="secondary"
            disabled={!selectedReleaseId || !canManage}
            onClick={() =>
              void perform(async () => {
                await api.data(`/api/repos/${repoId}/releases/${selectedReleaseId}/changesets`, {
                  method: "POST",
                  body: JSON.stringify({
                    changeset_ids: changesetIdsCsv
                      .split(",")
                      .map((id) => id.trim())
                      .filter(Boolean),
                  }),
                });
              })
            }
          >
            Set changesets
          </Button>
          <Button
            type="button"
            variant="secondary"
            disabled={!selectedReleaseId || !canManage}
            onClick={() =>
              void perform(async () => {
                await api.data(`/api/repos/${repoId}/releases/${selectedReleaseId}/reorder`, {
                  method: "POST",
                  body: JSON.stringify({ changeset_ids: JSON.parse(reorderBody) }),
                });
              })
            }
          >
            Reorder
          </Button>
          <Button
            type="button"
            variant="secondary"
            disabled={!selectedReleaseId || !canManage}
            onClick={() =>
              void perform(async () => {
                await api.data(`/api/repos/${repoId}/releases/${selectedReleaseId}/assemble`, {
                  method: "POST",
                  body: JSON.stringify({}),
                });
              })
            }
          >
            Assemble
          </Button>
          <Button
            type="button"
            disabled={!selectedReleaseId || !canManage}
            onClick={() =>
              void perform(async () => {
                await api.data(`/api/repos/${repoId}/releases/${selectedReleaseId}/publish`, {
                  method: "POST",
                  body: JSON.stringify({}),
                });
              })
            }
          >
            Publish
          </Button>
        </div>
      </Card>

      <Card className="space-y-3">
        <CardTitle>Releases</CardTitle>
        <div className="space-y-2">
          {(releasesQuery.data ?? []).map((release) => (
            <button
              key={release.id}
              type="button"
              className="w-full rounded-md border border-border bg-muted/30 p-2 text-left hover:bg-accent"
              onClick={() => setSelectedReleaseId(release.id)}
            >
              <p className="text-sm font-medium">{release.tag || release.id}</p>
              <p className="text-xs text-muted-foreground">state: {release.state}</p>
            </button>
          ))}
          {!releasesQuery.data?.length ? <p className="text-sm text-muted-foreground">No releases yet.</p> : null}
        </div>
      </Card>

      <RawDataPanel title="Advanced release payload" value={releasesQuery.data ?? []} />
    </Page>
  );
}

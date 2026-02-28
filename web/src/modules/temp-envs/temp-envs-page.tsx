import { FormEvent, useEffect, useMemo, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";

import { ApiError } from "@/api/client";
import { Button } from "@/components/ui/button";
import { Card, CardDescription, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { RawDataPanel } from "@/components/ui/raw-data-panel";
import { Select } from "@/components/ui/select";
import { StatusPill } from "@/components/ui/status-pill";
import { useApi } from "@/hooks/use-api";
import { useRepoContext } from "@/hooks/use-repo-context";
import { formatRoleLabel } from "@/lib/rbac";
import { formatDate } from "@/lib/utils";
import { Page } from "@/modules/shared/page";
import type { Changeset, Workspace } from "@/types/api";

interface TempEnvironment {
  id: string;
  kind: "workspace" | "changeset";
  workspace_id?: string | null;
  changeset_id?: string | null;
  source_id?: string | null;
  state: string;
  base_url?: string | null;
  expires_at?: string | null;
  updated_at: string;
}

export function TempEnvsPage(): React.ReactElement {
  const api = useApi();
  const queryClient = useQueryClient();
  const context = useRepoContext();
  const repoId = context?.repo?.id;
  const role = context?.role;

  const [kind, setKind] = useState<"workspace" | "changeset">("workspace");
  const [sourceId, setSourceId] = useState("");
  const [baseProfileId, setBaseProfileId] = useState("");
  const [selectedTempEnvId, setSelectedTempEnvId] = useState("");
  const [extendSeconds, setExtendSeconds] = useState("7200");
  const [status, setStatus] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const tempEnvQuery = useQuery({
    queryKey: ["temp-envs", repoId],
    queryFn: async () => {
      const envelope = await api.paginated<TempEnvironment[]>(`/api/repos/${repoId}/temp-envs?page=1&limit=100`);
      return envelope.data;
    },
    enabled: Boolean(repoId),
  });

  const workspacesQuery = useQuery({
    queryKey: ["temp-envs", "workspaces", repoId],
    queryFn: () => api.data<Workspace[]>(`/api/repos/${repoId}/workspaces`),
    enabled: Boolean(repoId),
  });

  const changesetsQuery = useQuery({
    queryKey: ["temp-envs", "changesets", repoId],
    queryFn: async () => {
      const envelope = await api.paginated<Changeset[]>(`/api/repos/${repoId}/changesets?page=1&limit=100`);
      return envelope.data;
    },
    enabled: Boolean(repoId),
  });

  const sourceOptions = useMemo(
    () =>
      kind === "workspace"
        ? (workspacesQuery.data ?? []).map((workspace) => ({
            id: workspace.id,
            label: workspace.title || workspace.branch_name,
          }))
        : (changesetsQuery.data ?? []).map((changeset) => ({
            id: changeset.id,
            label: `${changeset.title} (${changeset.state})`,
          })),
    [kind, workspacesQuery.data, changesetsQuery.data],
  );

  useEffect(() => {
    if (sourceOptions.length && !sourceOptions.some((option) => option.id === sourceId)) {
      setSourceId(sourceOptions[0].id);
    }
  }, [sourceOptions, sourceId]);

  useEffect(() => {
    if (!selectedTempEnvId && tempEnvQuery.data?.[0]?.id) {
      setSelectedTempEnvId(tempEnvQuery.data[0].id);
    }
  }, [selectedTempEnvId, tempEnvQuery.data]);

  const selectedTempEnv = useMemo(
    () => tempEnvQuery.data?.find((tempEnv) => tempEnv.id === selectedTempEnvId) ?? null,
    [tempEnvQuery.data, selectedTempEnvId],
  );

  const refresh = async (): Promise<void> => {
    await queryClient.invalidateQueries({ queryKey: ["temp-envs", repoId] });
    await queryClient.invalidateQueries({ queryKey: ["jobs", repoId] });
  };

  const createTempEnv = async (event: FormEvent<HTMLFormElement>): Promise<void> => {
    event.preventDefault();
    if (!repoId || !sourceId) return;
    setError(null);
    setStatus(null);
    try {
      await api.data(`/api/repos/${repoId}/temp-envs`, {
        method: "POST",
        body: JSON.stringify({
          kind,
          source_id: sourceId,
          base_profile_id: baseProfileId || null,
        }),
      });
      await refresh();
      setStatus("Temporary environment requested.");
    } catch (cause) {
      setError(cause instanceof ApiError ? cause.message : "failed to create temp env");
    }
  };

  const runAction = async (action: "extend" | "undo" | "delete"): Promise<void> => {
    if (!repoId || !selectedTempEnvId) return;
    setError(null);
    setStatus(null);
    try {
      if (action === "extend") {
        await api.data(`/api/repos/${repoId}/temp-envs/${selectedTempEnvId}/extend`, {
          method: "POST",
          body: JSON.stringify({ seconds: Number(extendSeconds) }),
        });
      } else if (action === "undo") {
        await api.data(`/api/repos/${repoId}/temp-envs/${selectedTempEnvId}/undo-expire`, {
          method: "POST",
          body: JSON.stringify({}),
        });
      } else {
        await api.data(`/api/repos/${repoId}/temp-envs/${selectedTempEnvId}`, {
          method: "DELETE",
        });
      }
      await refresh();
      setStatus(`Temp environment ${action} action completed.`);
    } catch (cause) {
      setError(cause instanceof ApiError ? cause.message : "temp env action failed");
    }
  };

  if (!repoId) {
    return <Page title="Temp Environments">Bind a repo first in Setup.</Page>;
  }

  return (
    <Page
      title="Temp Environments"
      description="Create shareable temporary preview environments from workspace or changeset state."
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
          You are signed in as {formatRoleLabel(role)}. Use temporary environments to validate behavior before queueing to
          release.
        </CardDescription>
      </Card>

      <div className="grid gap-4 xl:grid-cols-[340px_1fr]">
        <Card className="space-y-3">
          <CardTitle>Create Temp Environment</CardTitle>
          <form className="space-y-2" onSubmit={(event) => void createTempEnv(event)}>
            <Select value={kind} onChange={(event) => setKind(event.target.value as "workspace" | "changeset")}> 
              <option value="workspace">workspace source</option>
              <option value="changeset">changeset source</option>
            </Select>
            <Select value={sourceId} onChange={(event) => setSourceId(event.target.value)}>
              {sourceOptions.map((option) => (
                <option key={option.id} value={option.id}>
                  {option.label}
                </option>
              ))}
              {!sourceOptions.length ? <option value="">No available sources</option> : null}
            </Select>
            <Input
              value={baseProfileId}
              onChange={(event) => setBaseProfileId(event.target.value)}
              placeholder="base profile id (optional)"
            />
            <Button type="submit" disabled={!sourceId}>
              Create preview env
            </Button>
          </form>

          <Card className="border border-border/60 p-3">
            <CardTitle className="text-sm">Selected Env Actions</CardTitle>
            <CardDescription>Extend TTL, undo expiry, or delete selected environment.</CardDescription>
            <div className="mt-2 space-y-2">
              <Select value={selectedTempEnvId} onChange={(event) => setSelectedTempEnvId(event.target.value)}>
                <option value="">Select environment</option>
                {(tempEnvQuery.data ?? []).map((tempEnv) => (
                  <option key={tempEnv.id} value={tempEnv.id}>
                    {tempEnv.id} ({tempEnv.state})
                  </option>
                ))}
              </Select>
              <Input
                value={extendSeconds}
                onChange={(event) => setExtendSeconds(event.target.value)}
                placeholder="extend seconds"
              />
              <div className="flex flex-wrap gap-2">
                <Button type="button" variant="secondary" onClick={() => void runAction("extend")} disabled={!selectedTempEnvId}>
                  Extend TTL
                </Button>
                <Button type="button" variant="secondary" onClick={() => void runAction("undo")} disabled={!selectedTempEnvId}>
                  Undo Expire
                </Button>
                <Button type="button" variant="danger" onClick={() => void runAction("delete")} disabled={!selectedTempEnvId}>
                  Delete
                </Button>
              </div>
            </div>
          </Card>
        </Card>

        <Card className="space-y-3">
          <CardTitle>Active Temporary Environments</CardTitle>
          <div className="space-y-2">
            {(tempEnvQuery.data ?? []).map((tempEnv) => {
              const source = tempEnv.source_id || tempEnv.workspace_id || tempEnv.changeset_id || "-";
              const isSelected = selectedTempEnvId === tempEnv.id;
              return (
                <button
                  key={tempEnv.id}
                  type="button"
                  className={`w-full rounded-md border p-3 text-left transition-colors ${
                    isSelected ? "border-primary bg-primary/10" : "border-border bg-muted/30 hover:bg-accent"
                  }`}
                  onClick={() => setSelectedTempEnvId(tempEnv.id)}
                >
                  <div className="flex flex-wrap items-center justify-between gap-2">
                    <div>
                      <p className="text-sm font-semibold">{tempEnv.kind} · {tempEnv.id}</p>
                      <p className="text-xs text-muted-foreground">source {source}</p>
                    </div>
                    <StatusPill label={tempEnv.state} />
                  </div>
                  <div className="mt-2 space-y-1 text-xs text-muted-foreground">
                    <p>expires: {tempEnv.expires_at ? formatDate(tempEnv.expires_at) : "-"}</p>
                    <p>updated: {formatDate(tempEnv.updated_at)}</p>
                    <p>
                      url: {tempEnv.base_url ? (
                        <a className="text-primary underline" href={tempEnv.base_url} target="_blank" rel="noreferrer">
                          {tempEnv.base_url}
                        </a>
                      ) : (
                        "not ready"
                      )}
                    </p>
                  </div>
                </button>
              );
            })}
            {!tempEnvQuery.data?.length ? <p className="text-sm text-muted-foreground">No temp environments yet.</p> : null}
          </div>

          {selectedTempEnv?.base_url ? (
            <Button type="button" variant="outline" onClick={() => window.open(selectedTempEnv.base_url ?? "", "_blank")}> 
              Open selected preview URL
            </Button>
          ) : null}
        </Card>
      </div>

      <details>
        <summary className="cursor-pointer text-xs text-muted-foreground">Advanced temp environment payload</summary>
        <div className="mt-2">
          <RawDataPanel title="Temp environments payload" value={tempEnvQuery.data ?? []} />
        </div>
      </details>
    </Page>
  );
}

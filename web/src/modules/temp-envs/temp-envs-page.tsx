import { FormEvent, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";

import { ApiError } from "@/api/client";
import { Button } from "@/components/ui/button";
import { Card, CardDescription, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { RawDataPanel } from "@/components/ui/raw-data-panel";
import { Select } from "@/components/ui/select";
import { useApi } from "@/hooks/use-api";
import { useRepoContext } from "@/hooks/use-repo-context";
import { canManageReleases, formatRoleLabel } from "@/lib/rbac";
import { Page } from "@/modules/shared/page";

export function TempEnvsPage(): React.ReactElement {
  const api = useApi();
  const queryClient = useQueryClient();
  const context = useRepoContext();
  const repoId = context?.repo?.id;
  const role = context?.role;

  const [kind, setKind] = useState("workspace");
  const [sourceId, setSourceId] = useState("");
  const [baseProfileId, setBaseProfileId] = useState("");
  const [targetTempEnvId, setTargetTempEnvId] = useState("");
  const [extendSeconds, setExtendSeconds] = useState("7200");
  const [error, setError] = useState<string | null>(null);

  const canManage = canManageReleases(role);

  const tempEnvQuery = useQuery({
    queryKey: ["temp-envs", repoId],
    queryFn: () => api.data(`/api/repos/${repoId}/temp-envs`),
    enabled: Boolean(repoId),
  });

  const refresh = async (): Promise<void> => {
    await queryClient.invalidateQueries({ queryKey: ["temp-envs", repoId] });
    await queryClient.invalidateQueries({ queryKey: ["jobs", repoId] });
  };

  const createTempEnv = async (event: FormEvent<HTMLFormElement>): Promise<void> => {
    event.preventDefault();
    if (!repoId || !canManage) return;
    setError(null);
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
    } catch (cause) {
      setError(cause instanceof ApiError ? cause.message : "failed to create temp env");
    }
  };

  const runAction = async (action: "extend" | "undo" | "delete"): Promise<void> => {
    if (!repoId || !targetTempEnvId || !canManage) return;
    setError(null);
    try {
      if (action === "extend") {
        await api.data(`/api/repos/${repoId}/temp-envs/${targetTempEnvId}/extend`, {
          method: "POST",
          body: JSON.stringify({ seconds: Number(extendSeconds) }),
        });
      } else if (action === "undo") {
        await api.data(`/api/repos/${repoId}/temp-envs/${targetTempEnvId}/undo-expire`, {
          method: "POST",
          body: JSON.stringify({}),
        });
      } else {
        await api.data(`/api/repos/${repoId}/temp-envs/${targetTempEnvId}`, {
          method: "DELETE",
        });
      }
      await refresh();
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
      description="Create short-lived environments for workspace or changeset verification before release."
    >
      {error ? <Card className="border-destructive/40 bg-destructive/10 p-3 text-sm">{error}</Card> : null}

      <Card>
        <CardTitle>Role Scope</CardTitle>
        <CardDescription>
          You are signed in as {formatRoleLabel(role)}.
          {canManage
            ? " You can create and manage temporary environments."
            : " Temporary environment actions require Config Manager or above."}
        </CardDescription>
      </Card>

      <div className="grid gap-4 lg:grid-cols-2">
        <Card>
          <CardTitle>Create Temp Env</CardTitle>
          <form className="mt-3 space-y-2" onSubmit={(event) => void createTempEnv(event)}>
            <Select value={kind} onChange={(event) => setKind(event.target.value)} disabled={!canManage}>
              <option value="workspace">workspace</option>
              <option value="changeset">changeset</option>
            </Select>
            <Input
              value={sourceId}
              onChange={(event) => setSourceId(event.target.value)}
              placeholder="source id"
              required
              disabled={!canManage}
            />
            <Input
              value={baseProfileId}
              onChange={(event) => setBaseProfileId(event.target.value)}
              placeholder="base profile id (optional)"
              disabled={!canManage}
            />
            <Button type="submit" disabled={!canManage}>Create</Button>
          </form>
        </Card>

        <Card>
          <CardTitle>Temp Env Actions</CardTitle>
          <div className="mt-3 space-y-2">
            <Input
              value={targetTempEnvId}
              onChange={(event) => setTargetTempEnvId(event.target.value)}
              placeholder="temp env id"
              disabled={!canManage}
            />
            <Input
              value={extendSeconds}
              onChange={(event) => setExtendSeconds(event.target.value)}
              placeholder="extend seconds"
              disabled={!canManage}
            />
            <div className="flex flex-wrap gap-2">
              <Button type="button" variant="secondary" onClick={() => void runAction("extend")} disabled={!canManage}>Extend TTL</Button>
              <Button type="button" variant="secondary" onClick={() => void runAction("undo")} disabled={!canManage}>Undo Expire</Button>
              <Button type="button" variant="danger" onClick={() => void runAction("delete")} disabled={!canManage}>Delete</Button>
            </div>
          </div>
        </Card>
      </div>

      <RawDataPanel title="Current temp environments" value={tempEnvQuery.data ?? []} />
    </Page>
  );
}

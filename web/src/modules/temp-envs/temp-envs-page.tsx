import { FormEvent, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";

import { ApiError } from "@/api/client";
import { Button } from "@/components/ui/button";
import { Card, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Select } from "@/components/ui/select";
import { useApi } from "@/hooks/use-api";
import { useRepoContext } from "@/hooks/use-repo-context";
import { JsonView } from "@/components/ui/json-view";
import { Page } from "@/modules/shared/page";

export function TempEnvsPage(): React.ReactElement {
  const api = useApi();
  const queryClient = useQueryClient();
  const context = useRepoContext();
  const repoId = context?.repo?.id;

  const [kind, setKind] = useState("workspace");
  const [sourceId, setSourceId] = useState("");
  const [baseProfileId, setBaseProfileId] = useState("");
  const [targetTempEnvId, setTargetTempEnvId] = useState("");
  const [extendSeconds, setExtendSeconds] = useState("7200");
  const [error, setError] = useState<string | null>(null);

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
    if (!repoId) return;
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
    if (!repoId || !targetTempEnvId) return;
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
    <Page title="Temp Environments" description="Create and manage workspace/changeset ephemeral environments.">
      {error ? <Card className="border-destructive/40 bg-destructive/10 p-3 text-sm">{error}</Card> : null}

      <div className="grid gap-4 lg:grid-cols-2">
        <Card>
          <CardTitle>Create Temp Env</CardTitle>
          <form className="mt-3 space-y-2" onSubmit={(event) => void createTempEnv(event)}>
            <Select value={kind} onChange={(event) => setKind(event.target.value)}>
              <option value="workspace">workspace</option>
              <option value="changeset">changeset</option>
            </Select>
            <Input value={sourceId} onChange={(event) => setSourceId(event.target.value)} placeholder="source id" required />
            <Input
              value={baseProfileId}
              onChange={(event) => setBaseProfileId(event.target.value)}
              placeholder="base profile id (optional)"
            />
            <Button type="submit">Create</Button>
          </form>
        </Card>

        <Card>
          <CardTitle>Actions</CardTitle>
          <div className="mt-3 space-y-2">
            <Input
              value={targetTempEnvId}
              onChange={(event) => setTargetTempEnvId(event.target.value)}
              placeholder="temp env id"
            />
            <Input value={extendSeconds} onChange={(event) => setExtendSeconds(event.target.value)} placeholder="extend seconds" />
            <div className="flex flex-wrap gap-2">
              <Button type="button" variant="secondary" onClick={() => void runAction("extend")}>Extend TTL</Button>
              <Button type="button" variant="secondary" onClick={() => void runAction("undo")}>Undo Expire</Button>
              <Button type="button" variant="danger" onClick={() => void runAction("delete")}>Delete</Button>
            </div>
          </div>
        </Card>
      </div>

      <Card>
        <CardTitle>Current Temp Environments</CardTitle>
        <div className="mt-3">
          <JsonView value={tempEnvQuery.data ?? []} />
        </div>
      </Card>
    </Page>
  );
}

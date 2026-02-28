import { useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";

import { ApiError } from "@/api/client";
import { Button } from "@/components/ui/button";
import { Card, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Select } from "@/components/ui/select";
import { Textarea } from "@/components/ui/textarea";
import { useApi } from "@/hooks/use-api";
import { useRepoContext } from "@/hooks/use-repo-context";
import { JsonView } from "@/components/ui/json-view";
import { StatusPill } from "@/components/ui/status-pill";
import { Page } from "@/modules/shared/page";
import type { Deployment } from "@/types/api";

export function DeploymentsPage(): React.ReactElement {
  const api = useApi();
  const queryClient = useQueryClient();
  const context = useRepoContext();
  const repoId = context?.repo?.id;

  const [environmentId, setEnvironmentId] = useState("");
  const [releaseId, setReleaseId] = useState("");
  const [approvalsCsv, setApprovalsCsv] = useState("");
  const [rollbackMode, setRollbackMode] = useState("revert_and_release");
  const [action, setAction] = useState<"deploy" | "promote" | "rollback">("deploy");
  const [error, setError] = useState<string | null>(null);

  const environmentsQuery = useQuery({
    queryKey: ["environments", repoId],
    queryFn: () => api.data<Array<{ id: string; name: string }>>(`/api/repos/${repoId}/environments`),
    enabled: Boolean(repoId),
  });

  const deploymentsQuery = useQuery({
    queryKey: ["deployments", repoId],
    queryFn: () => api.data<Deployment[]>(`/api/repos/${repoId}/deployments`),
    enabled: Boolean(repoId),
    refetchInterval: 3000,
  });

  const runAction = async (): Promise<void> => {
    if (!repoId || !environmentId || !releaseId) return;
    setError(null);
    try {
      if (action === "rollback") {
        await api.data(`/api/repos/${repoId}/environments/${environmentId}/rollback`, {
          method: "POST",
          body: JSON.stringify({
            release_id: releaseId,
            mode: rollbackMode,
            approvals: approvalsCsv
              .split(",")
              .map((value) => value.trim())
              .filter(Boolean),
          }),
        });
      } else {
        const endpoint = action === "deploy" ? "deploy" : "promote";
        await api.data(`/api/repos/${repoId}/environments/${environmentId}/${endpoint}`, {
          method: "POST",
          body: JSON.stringify({
            release_id: releaseId,
            is_skip_stage: false,
            is_concurrent_batch: false,
            approvals: approvalsCsv
              .split(",")
              .map((value) => value.trim())
              .filter(Boolean),
          }),
        });
      }
      await queryClient.invalidateQueries({ queryKey: ["deployments", repoId] });
      await queryClient.invalidateQueries({ queryKey: ["jobs", repoId] });
    } catch (cause) {
      setError(cause instanceof ApiError ? cause.message : "deployment action failed");
    }
  };

  if (!repoId) {
    return <Page title="Deployments">Bind a repo first in Setup.</Page>;
  }

  return (
    <Page title="Deployments" description="Deploy, promote, or rollback releases across environments.">
      {error ? <Card className="border-destructive/40 bg-destructive/10 p-3 text-sm">{error}</Card> : null}

      <Card className="space-y-3">
        <CardTitle>Deployment Action</CardTitle>
        <div className="grid gap-2 lg:grid-cols-4">
          <Select value={action} onChange={(event) => setAction(event.target.value as typeof action)}>
            <option value="deploy">deploy</option>
            <option value="promote">promote</option>
            <option value="rollback">rollback</option>
          </Select>
          <Select value={environmentId} onChange={(event) => setEnvironmentId(event.target.value)}>
            <option value="">environment</option>
            {(environmentsQuery.data ?? []).map((environment) => (
              <option key={environment.id} value={environment.id}>
                {environment.name}
              </option>
            ))}
          </Select>
          <Input value={releaseId} onChange={(event) => setReleaseId(event.target.value)} placeholder="release id" />
          <Button type="button" onClick={() => void runAction()} disabled={!environmentId || !releaseId}>
            Execute
          </Button>
        </div>
        <Input value={approvalsCsv} onChange={(event) => setApprovalsCsv(event.target.value)} placeholder="approver user ids csv" />
        {action === "rollback" ? (
          <Textarea value={rollbackMode} onChange={(event) => setRollbackMode(event.target.value)} className="min-h-16" />
        ) : null}
      </Card>

      <Card className="space-y-3">
        <CardTitle>Deployments</CardTitle>
        <div className="space-y-2">
          {(deploymentsQuery.data ?? []).map((deployment) => (
            <div key={deployment.id} className="bg-muted flex items-center justify-between rounded-md p-2">
              <div className="text-xs">
                <p className="font-semibold">{deployment.id}</p>
                <p className="text-muted-foreground">env={deployment.environment_id} release={deployment.release_id}</p>
              </div>
              <StatusPill label={deployment.state} />
            </div>
          ))}
        </div>
        <JsonView value={deploymentsQuery.data ?? []} />
      </Card>
    </Page>
  );
}

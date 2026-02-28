import { useMemo, useState } from "react";
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
import { canManageReleases, formatRoleLabel } from "@/lib/rbac";
import { formatDate } from "@/lib/utils";
import {
  countDeploymentHistoryByState,
  filterDeploymentHistory,
  type DeploymentHistoryFilterState,
} from "@/modules/deployments/deployments-utils";
import { Page } from "@/modules/shared/page";
import type { Deployment, ReleaseBatch } from "@/types/api";

type DeploymentAction = "deploy" | "promote" | "rollback";
const activeDeploymentStates = new Set(["pending", "running"]);

export function DeploymentsPage(): React.ReactElement {
  const api = useApi();
  const queryClient = useQueryClient();
  const context = useRepoContext();
  const repoId = context?.repo?.id;
  const role = context?.role;

  const [environmentId, setEnvironmentId] = useState("");
  const [releaseId, setReleaseId] = useState("");
  const [approvalsCsv, setApprovalsCsv] = useState("");
  const [rollbackMode, setRollbackMode] = useState("revert_and_release");
  const [action, setAction] = useState<DeploymentAction>("deploy");
  const [historyEnvironmentFilter, setHistoryEnvironmentFilter] = useState("all");
  const [historyStateFilter, setHistoryStateFilter] = useState<DeploymentHistoryFilterState>("all");
  const [historySearch, setHistorySearch] = useState("");
  const [selectedDeploymentId, setSelectedDeploymentId] = useState("");
  const [status, setStatus] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const canManage = canManageReleases(role);

  const environmentsQuery = useQuery({
    queryKey: ["environments", repoId],
    queryFn: () => api.data<Array<{ id: string; name: string }>>(`/api/repos/${repoId}/environments`),
    enabled: Boolean(repoId),
  });

  const releasesQuery = useQuery({
    queryKey: ["releases", repoId],
    queryFn: async () => {
      const envelope = await api.paginated<ReleaseBatch[]>(`/api/repos/${repoId}/releases?page=1&limit=100`);
      return envelope.data;
    },
    enabled: Boolean(repoId),
  });

  const deploymentsQuery = useQuery({
    queryKey: ["deployments", repoId],
    queryFn: () => api.data<Deployment[]>(`/api/repos/${repoId}/deployments`),
    enabled: Boolean(repoId),
    refetchInterval: (query) => {
      const deployments = query.state.data ?? [];
      if (!deployments.length) return false;
      return deployments.some((deployment) => activeDeploymentStates.has(deployment.state.toLowerCase())) ? 3000 : 12000;
    },
    refetchIntervalInBackground: false,
  });

  const environmentNameById = useMemo(
    () => new Map((environmentsQuery.data ?? []).map((environment) => [environment.id, environment.name])),
    [environmentsQuery.data],
  );

  const latestByEnvironment = useMemo(() => {
    const map = new Map<string, Deployment>();
    for (const deployment of deploymentsQuery.data ?? []) {
      const current = map.get(deployment.environment_id);
      if (!current || deployment.created_at > current.created_at) {
        map.set(deployment.environment_id, deployment);
      }
    }
    return map;
  }, [deploymentsQuery.data]);

  const historyCounts = useMemo(
    () => countDeploymentHistoryByState(deploymentsQuery.data ?? []),
    [deploymentsQuery.data],
  );

  const historyItems = useMemo(
    () =>
      filterDeploymentHistory(
        deploymentsQuery.data ?? [],
        historyEnvironmentFilter,
        historyStateFilter,
        historySearch,
      ),
    [deploymentsQuery.data, historyEnvironmentFilter, historyStateFilter, historySearch],
  );

  const selectedDeployment = useMemo(
    () => historyItems.find((deployment) => deployment.id === selectedDeploymentId) ?? historyItems[0] ?? null,
    [historyItems, selectedDeploymentId],
  );

  const parseApprovals = (): string[] =>
    approvalsCsv
      .split(",")
      .map((value) => value.trim())
      .filter(Boolean);

  const runAction = async (): Promise<void> => {
    if (!repoId || !environmentId || !releaseId || !canManage) return;
    setError(null);
    setStatus(null);
    try {
      if (action === "rollback") {
        await api.data(`/api/repos/${repoId}/environments/${environmentId}/rollback`, {
          method: "POST",
          body: JSON.stringify({
            release_id: releaseId,
            mode: rollbackMode,
            approvals: parseApprovals(),
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
            approvals: parseApprovals(),
          }),
        });
      }
      await queryClient.invalidateQueries({ queryKey: ["deployments", repoId] });
      await queryClient.invalidateQueries({ queryKey: ["jobs", repoId] });
      setStatus(`${action} started for selected environment.`);
    } catch (cause) {
      setError(cause instanceof ApiError ? cause.message : "deployment action failed");
    }
  };

  if (!repoId) {
    return <Page title="Deployments">Bind a repo first in Setup.</Page>;
  }

  return (
    <Page
      title="Deployments"
      description="Deploy and promote published releases across environments with clear pipeline visibility."
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
            ? " You can deploy, promote, and rollback releases."
            : " You can view deployment history only."}
        </CardDescription>
      </Card>

      <Card className="space-y-3">
        <CardTitle>Environment Pipeline Snapshot</CardTitle>
        <div className="grid gap-3 lg:grid-cols-3">
          {(environmentsQuery.data ?? []).map((environment) => {
            const deployment = latestByEnvironment.get(environment.id);
            return (
              <Card key={environment.id} className="border border-border/60 p-3">
                <div className="flex items-center justify-between gap-2">
                  <h3 className="text-sm font-semibold">{environment.name}</h3>
                  {deployment ? <StatusPill label={deployment.state} /> : <StatusPill label="no deploy" />}
                </div>
                <p className="mt-2 text-xs text-muted-foreground">
                  {deployment ? `release ${deployment.release_id}` : "No deployment recorded yet."}
                </p>
                {deployment ? (
                  <p className="text-xs text-muted-foreground">updated {formatDate(deployment.updated_at)}</p>
                ) : null}
              </Card>
            );
          })}
        </div>
      </Card>

      <Card className="space-y-3">
        <CardTitle>Deployment Action</CardTitle>
        <CardDescription>Pick environment and release, then execute deploy/promotion action.</CardDescription>
        <div className="grid gap-2 lg:grid-cols-4">
          <Select id="deployment-action" label="Action" value={action} onChange={(event) => setAction(event.target.value as DeploymentAction)}>
            <option value="deploy">deploy</option>
            <option value="promote">promote</option>
            <option value="rollback">rollback</option>
          </Select>
          <Select id="deployment-environment" label="Environment" value={environmentId} onChange={(event) => setEnvironmentId(event.target.value)}>
            <option value="">environment</option>
            {(environmentsQuery.data ?? []).map((environment) => (
              <option key={environment.id} value={environment.id}>
                {environment.name}
              </option>
            ))}
          </Select>
          <Select id="deployment-release" label="Release" value={releaseId} onChange={(event) => setReleaseId(event.target.value)}>
            <option value="">release</option>
            {(releasesQuery.data ?? []).map((release) => (
              <option key={release.id} value={release.id}>
                {release.tag || release.id}
              </option>
            ))}
          </Select>
          <Button type="button" onClick={() => void runAction()} disabled={!environmentId || !releaseId || !canManage}>
            Execute
          </Button>
        </div>

        {action === "rollback" ? (
          <div className="grid gap-2 lg:grid-cols-[200px_1fr]">
            <label className="self-center text-xs text-muted-foreground" htmlFor="rollback-mode-select">
              Rollback mode
            </label>
            <Select id="rollback-mode-select" value={rollbackMode} onChange={(event) => setRollbackMode(event.target.value)}>
              <option value="revert_and_release">revert_and_release</option>
              <option value="redeploy_previous">redeploy_previous</option>
            </Select>
          </div>
        ) : null}

        <details>
          <summary className="cursor-pointer text-xs text-muted-foreground">Advanced approvals</summary>
          <Input
            className="mt-2"
            value={approvalsCsv}
            onChange={(event) => setApprovalsCsv(event.target.value)}
            placeholder="approver user ids csv"
            disabled={!canManage}
          />
        </details>
      </Card>

      <Card className="space-y-3">
        <div className="flex items-center justify-between gap-2">
          <CardTitle>Deployment History</CardTitle>
          <div className="flex items-center gap-2">
            <Select id="history-environment-filter" label="Environment filter" value={historyEnvironmentFilter} onChange={(event) => setHistoryEnvironmentFilter(event.target.value)}>
              <option value="all">all environments</option>
              {(environmentsQuery.data ?? []).map((environment) => (
                <option key={environment.id} value={environment.id}>
                  {environment.name}
                </option>
              ))}
            </Select>
            <Select
              id="history-state-filter"
              label="State filter"
              value={historyStateFilter}
              onChange={(event) => setHistoryStateFilter(event.target.value as DeploymentHistoryFilterState)}
            >
              <option value="all">all states ({historyCounts.all})</option>
              <option value="running">running ({historyCounts.running})</option>
              <option value="succeeded">succeeded ({historyCounts.succeeded})</option>
              <option value="failed">failed ({historyCounts.failed})</option>
            </Select>
            <Input
              id="history-search"
              label="Search"
              value={historySearch}
              onChange={(event) => setHistorySearch(event.target.value)}
              placeholder="search by id/release/state"
            />
          </div>
        </div>

        <div className="grid gap-4 lg:grid-cols-[1fr_320px]">
          <div className="space-y-2">
          {historyItems.map((deployment) => (
            <button
              key={deployment.id}
              type="button"
              onClick={() => setSelectedDeploymentId(deployment.id)}
              className={`w-full rounded-md border p-3 text-left transition-colors ${
                selectedDeploymentId === deployment.id
                  ? "border-primary bg-primary/10"
                  : "border-border bg-muted/30 hover:bg-accent"
              }`}
            >
              <div className="flex flex-wrap items-center justify-between gap-2">
                <span className="text-sm font-medium">{environmentNameById.get(deployment.environment_id) || deployment.environment_id}</span>
                <StatusPill label={deployment.state} />
              </div>
              <p className="mt-1 text-xs text-muted-foreground">
                release {deployment.release_id} · {formatDate(deployment.created_at)}
              </p>
              <p className="text-xs text-muted-foreground">id: {deployment.id}</p>
            </button>
          ))}
          {!historyItems.length ? <p className="text-sm text-muted-foreground">No deployments recorded yet.</p> : null}
          </div>
          <Card className="space-y-2 border border-border/60 p-3">
            <CardTitle className="text-sm">Selected Deployment</CardTitle>
            {!selectedDeployment ? (
              <p className="text-sm text-muted-foreground">Select a deployment to inspect details.</p>
            ) : (
              <>
                <p className="text-xs text-muted-foreground">id: {selectedDeployment.id}</p>
                <p className="text-xs text-muted-foreground">
                  environment: {environmentNameById.get(selectedDeployment.environment_id) || selectedDeployment.environment_id}
                </p>
                <p className="text-xs text-muted-foreground">release: {selectedDeployment.release_id}</p>
                <p className="text-xs text-muted-foreground">state: {selectedDeployment.state}</p>
                <p className="text-xs text-muted-foreground">created: {formatDate(selectedDeployment.created_at)}</p>
                <p className="text-xs text-muted-foreground">updated: {formatDate(selectedDeployment.updated_at)}</p>
              </>
            )}
          </Card>
        </div>
      </Card>

      <details>
        <summary className="cursor-pointer text-xs text-muted-foreground">Advanced deployment payload</summary>
        <div className="mt-2">
          <RawDataPanel title="Deployments payload" value={deploymentsQuery.data ?? []} />
        </div>
      </details>
    </Page>
  );
}

import { FormEvent, useEffect, useMemo, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";

import { ApiError } from "@/api/client";
import { Button } from "@/components/ui/button";
import { Card, CardDescription, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Select } from "@/components/ui/select";
import { useApi } from "@/hooks/use-api";
import { useRepoContext } from "@/hooks/use-repo-context";
import { canManageAdministration, formatRoleLabel } from "@/lib/rbac";
import { Page } from "@/modules/shared/page";
import type { Repo, Team } from "@/types/api";

export function SettingsPage(): React.ReactElement {
  const api = useApi();
  const queryClient = useQueryClient();
  const context = useRepoContext();

  const [repoId, setRepoId] = useState(context?.repo?.id ?? "");
  const [selectedTeamId, setSelectedTeamId] = useState(context?.team?.id ?? "");
  const [status, setStatus] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const canManage = canManageAdministration(context?.role);

  const teamsQuery = useQuery({
    queryKey: ["settings", "teams"],
    queryFn: async () => {
      const envelope = await api.paginated<Team[]>("/api/teams?page=1&limit=500");
      return envelope.data;
    },
    enabled: canManage,
  });

  const reposQuery = useQuery({
    queryKey: ["settings", "repos"],
    queryFn: async () => {
      const envelope = await api.paginated<Repo[]>("/api/repos?page=1&limit=500");
      return envelope.data;
    },
    enabled: canManage,
  });

  const filteredRepos = useMemo(() => {
    const repos = reposQuery.data ?? [];
    if (!selectedTeamId) {
      return repos;
    }
    return repos.filter((repo) => repo.team_id === selectedTeamId);
  }, [reposQuery.data, selectedTeamId]);

  useEffect(() => {
    if (!repoId && filteredRepos[0]?.id) {
      setRepoId(filteredRepos[0].id);
    }
  }, [repoId, filteredRepos]);

  const selectedRepo = useMemo(
    () => (reposQuery.data ?? []).find((repo) => repo.id === repoId) ?? null,
    [reposQuery.data, repoId],
  );

  const onSubmit = async (event: FormEvent<HTMLFormElement>): Promise<void> => {
    event.preventDefault();
    if (!canManage || !repoId) return;
    setStatus(null);
    setError(null);
    try {
      await api.data("/api/repo", {
        method: "PATCH",
        body: JSON.stringify({ repo_id: repoId }),
      });
      await queryClient.invalidateQueries({ queryKey: ["repo-context"] });
      setStatus("Bound instance updated.");
    } catch (cause) {
      setError(cause instanceof ApiError ? cause.message : "failed to update bound instance");
    }
  };

  return (
    <Page title="Settings" description="Administration settings for this console instance.">
      <Card>
        <CardTitle>Role Scope</CardTitle>
        <CardDescription>
          You are signed in as {formatRoleLabel(context?.role)}.
          {canManage ? " You can rebind the console to another instance." : " Settings updates require Admin or Owner."}
        </CardDescription>
      </Card>

      <Card>
        <CardTitle>Current Instance</CardTitle>
        <CardDescription>
          {context?.repo ? `${context.repo.name} (${context.repo.id})` : "No instance currently bound"}
        </CardDescription>
      </Card>

      <Card>
        <CardTitle>Rebind Instance</CardTitle>
        <CardDescription>Prefer selecting by team and instance name instead of raw IDs.</CardDescription>
        <form className="mt-3 space-y-3" onSubmit={(event) => void onSubmit(event)}>
          <Select
            id="settings-team-select"
            label="Team"
            value={selectedTeamId}
            onChange={(event) => {
              setSelectedTeamId(event.target.value);
              setRepoId("");
            }}
            disabled={!canManage}
          >
            <option value="">All teams</option>
            {(teamsQuery.data ?? []).map((team) => (
              <option key={team.id} value={team.id}>
                {team.name}
              </option>
            ))}
          </Select>

          <Select
            id="settings-repo-select"
            label="Instance"
            value={repoId}
            onChange={(event) => setRepoId(event.target.value)}
            disabled={!canManage}
          >
            <option value="">Select instance</option>
            {filteredRepos.map((repo) => (
              <option key={repo.id} value={repo.id}>
                {repo.name} ({repo.repo_path})
              </option>
            ))}
          </Select>

          {selectedRepo ? (
            <div className="rounded-md border border-border bg-muted/30 p-3 text-xs text-muted-foreground">
              <p>id: {selectedRepo.id}</p>
              <p>repo path: {selectedRepo.repo_path}</p>
              <p>integration branch: {selectedRepo.integration_branch}</p>
            </div>
          ) : null}

          <details>
            <summary className="cursor-pointer text-xs text-muted-foreground">Advanced: set instance by id</summary>
            <Input
              className="mt-2"
              value={repoId}
              onChange={(event) => setRepoId(event.target.value)}
              placeholder="instance id"
              disabled={!canManage}
            />
          </details>

          <Button type="submit" disabled={!context?.can_rebind || !canManage || !repoId}>
            Apply instance binding
          </Button>
        </form>
      </Card>

      {status ? (
        <Card className="border-success/40 bg-success/40 p-3 text-sm" aria-live="polite">
          {status}
        </Card>
      ) : null}
      {error ? <Card className="border-destructive/40 bg-destructive/10 p-3 text-sm">{error}</Card> : null}
    </Page>
  );
}

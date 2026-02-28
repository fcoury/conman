import { FormEvent, useMemo, useState } from "react";
import { useNavigate } from "react-router-dom";
import { useQuery, useQueryClient } from "@tanstack/react-query";

import { ApiError } from "@/api/client";
import { Button } from "@/components/ui/button";
import { Card, CardDescription, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Select } from "@/components/ui/select";
import { useApi } from "@/hooks/use-api";
import { useRepoContext } from "@/hooks/use-repo-context";
import { Page } from "@/modules/shared/page";
import type { Repo, Team } from "@/types/api";

function randomSlug(prefix: string): string {
  const random = Math.random().toString(36).slice(2, 8);
  return `${prefix}-${random}`;
}

export function SetupPage(): React.ReactElement {
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const api = useApi();
  const repoContext = useRepoContext();

  const [selectedTeamId, setSelectedTeamId] = useState("");
  const [selectedRepoId, setSelectedRepoId] = useState("");

  const [newTeamName, setNewTeamName] = useState("Conman Team");
  const [newTeamSlug, setNewTeamSlug] = useState(randomSlug("team"));

  const [newRepoName, setNewRepoName] = useState("Team Configuration");
  const [newRepoPath, setNewRepoPath] = useState(randomSlug("team-config"));
  const [newRepoIntegrationBranch, setNewRepoIntegrationBranch] = useState("main");

  const [newAppKey, setNewAppKey] = useState("portal");
  const [newAppTitle, setNewAppTitle] = useState("Primary App");
  const [newAppDomain, setNewAppDomain] = useState("portal.example.test");

  const [statusMessage, setStatusMessage] = useState<string | null>(null);
  const [errorMessage, setErrorMessage] = useState<string | null>(null);

  const teamsQuery = useQuery({
    queryKey: ["setup", "teams"],
    queryFn: async () => {
      const envelope = await api.paginated<Team[]>("/api/teams?page=1&limit=100");
      return envelope.data;
    },
  });

  const reposQuery = useQuery({
    queryKey: ["setup", "repos"],
    queryFn: async () => {
      const envelope = await api.paginated<Repo[]>("/api/repos?page=1&limit=100");
      return envelope.data;
    },
  });

  const teams = teamsQuery.data ?? [];
  const repos = reposQuery.data ?? [];

  const effectiveTeamId = useMemo(() => {
    if (selectedTeamId) return selectedTeamId;
    return teams[0]?.id ?? "";
  }, [selectedTeamId, teams]);

  const effectiveRepoId = useMemo(() => {
    if (selectedRepoId) return selectedRepoId;
    return repos[0]?.id ?? "";
  }, [selectedRepoId, repos]);

  const setError = (cause: unknown): void => {
    setErrorMessage(cause instanceof ApiError ? cause.message : "request failed");
  };

  const refresh = async (): Promise<void> => {
    await Promise.all([teamsQuery.refetch(), reposQuery.refetch(), queryClient.invalidateQueries({ queryKey: ["repo-context"] })]);
  };

  const createTeam = async (event: FormEvent<HTMLFormElement>): Promise<void> => {
    event.preventDefault();
    setErrorMessage(null);
    setStatusMessage(null);
    try {
      const created = await api.data<Team>("/api/teams", {
        method: "POST",
        body: JSON.stringify({ name: newTeamName, slug: newTeamSlug }),
      });
      setSelectedTeamId(created.id);
      setStatusMessage(`Created team ${created.name}`);
      await refresh();
    } catch (cause) {
      setError(cause);
    }
  };

  const createRepo = async (event: FormEvent<HTMLFormElement>): Promise<void> => {
    event.preventDefault();
    if (!effectiveTeamId) {
      setErrorMessage("Select or create a team first");
      return;
    }
    setErrorMessage(null);
    setStatusMessage(null);
    try {
      const created = await api.data<Repo>(`/api/teams/${effectiveTeamId}/repos`, {
        method: "POST",
        body: JSON.stringify({
          name: newRepoName,
          repo_path: newRepoPath,
          integration_branch: newRepoIntegrationBranch,
        }),
      });
      setSelectedRepoId(created.id);
      setStatusMessage(`Created repo ${created.name}`);
      await refresh();
    } catch (cause) {
      setError(cause);
    }
  };

  const createApp = async (event: FormEvent<HTMLFormElement>): Promise<void> => {
    event.preventDefault();
    if (!effectiveRepoId) {
      setErrorMessage("Select or create a repo first");
      return;
    }
    setErrorMessage(null);
    setStatusMessage(null);
    try {
      await api.data(`/api/repos/${effectiveRepoId}/apps`, {
        method: "POST",
        body: JSON.stringify({
          key: newAppKey,
          title: newAppTitle,
          domains: newAppDomain ? [newAppDomain] : [],
        }),
      });
      setStatusMessage("Created app");
      await refresh();
    } catch (cause) {
      setError(cause);
    }
  };

  const bindRepo = async (repoId: string): Promise<void> => {
    setErrorMessage(null);
    setStatusMessage(null);
    try {
      await api.data("/api/repo", {
        method: "PATCH",
        body: JSON.stringify({ repo_id: repoId }),
      });
      await queryClient.invalidateQueries({ queryKey: ["repo-context"] });
      setStatusMessage("Bound repo successfully");
      navigate("/workspaces", { replace: true });
    } catch (cause) {
      setError(cause);
    }
  };

  return (
    <Page title="Setup" description="Create or bind the single repo instance context for this Conman UI.">
      {repoContext?.status === "bound" ? (
        <Card className="border-success/40 bg-success/40 p-4">
          <CardTitle>Instance already bound</CardTitle>
          <CardDescription>
            Current bound repo: <strong>{repoContext.repo?.name ?? repoContext.binding?.repo_id}</strong>
          </CardDescription>
        </Card>
      ) : null}

      {errorMessage ? (
        <Card className="border-destructive/40 bg-destructive/10 p-4 text-sm">{errorMessage}</Card>
      ) : null}
      {statusMessage ? <Card className="border-success/40 bg-success/40 p-4 text-sm">{statusMessage}</Card> : null}

      <Card className="space-y-3">
        <CardTitle>Bind Existing Repository</CardTitle>
        <div className="flex flex-wrap items-end gap-2">
          <div className="min-w-72 flex-1">
            <label className="text-muted-foreground mb-1 block text-xs">Repository</label>
            <Select value={effectiveRepoId} onChange={(event) => setSelectedRepoId(event.target.value)}>
              <option value="">Select repository...</option>
              {repos.map((repo) => (
                <option key={repo.id} value={repo.id}>
                  {repo.name} ({repo.id})
                </option>
              ))}
            </Select>
          </div>
          <Button type="button" onClick={() => void bindRepo(effectiveRepoId)} disabled={!effectiveRepoId}>
            Bind selected repo
          </Button>
        </div>
      </Card>

      <div className="grid gap-4 lg:grid-cols-3">
        <Card>
          <CardTitle>Create Team</CardTitle>
          <form className="mt-3 space-y-2" onSubmit={(event) => void createTeam(event)}>
            <Input value={newTeamName} onChange={(event) => setNewTeamName(event.target.value)} placeholder="Team name" required />
            <Input value={newTeamSlug} onChange={(event) => setNewTeamSlug(event.target.value)} placeholder="Slug" required />
            <Button type="submit">Create team</Button>
          </form>
        </Card>

        <Card>
          <CardTitle>Create Repo</CardTitle>
          <form className="mt-3 space-y-2" onSubmit={(event) => void createRepo(event)}>
            <Select value={effectiveTeamId} onChange={(event) => setSelectedTeamId(event.target.value)}>
              <option value="">Select team...</option>
              {teams.map((team) => (
                <option key={team.id} value={team.id}>
                  {team.name}
                </option>
              ))}
            </Select>
            <Input value={newRepoName} onChange={(event) => setNewRepoName(event.target.value)} placeholder="Repo name" required />
            <Input value={newRepoPath} onChange={(event) => setNewRepoPath(event.target.value)} placeholder="Repo path" required />
            <Input
              value={newRepoIntegrationBranch}
              onChange={(event) => setNewRepoIntegrationBranch(event.target.value)}
              placeholder="Integration branch"
              required
            />
            <Button type="submit" disabled={!effectiveTeamId}>
              Create repo
            </Button>
          </form>
        </Card>

        <Card>
          <CardTitle>Create First App</CardTitle>
          <form className="mt-3 space-y-2" onSubmit={(event) => void createApp(event)}>
            <Select value={effectiveRepoId} onChange={(event) => setSelectedRepoId(event.target.value)}>
              <option value="">Select repo...</option>
              {repos.map((repo) => (
                <option key={repo.id} value={repo.id}>
                  {repo.name}
                </option>
              ))}
            </Select>
            <Input value={newAppKey} onChange={(event) => setNewAppKey(event.target.value)} placeholder="App key" required />
            <Input value={newAppTitle} onChange={(event) => setNewAppTitle(event.target.value)} placeholder="App title" required />
            <Input value={newAppDomain} onChange={(event) => setNewAppDomain(event.target.value)} placeholder="Domain" />
            <Button type="submit" disabled={!effectiveRepoId}>
              Create app
            </Button>
          </form>
        </Card>
      </div>
    </Page>
  );
}

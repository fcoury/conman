import { FormEvent, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Select } from "@/components/ui/select";
import { useApi } from "@/hooks/use-api";
import type { Repo } from "@/types/api";
import { ApiError } from "@/api/client";

interface RepoStepProps {
  teamId: string;
  selectedRepoId: string;
  onSelect: (repoId: string) => void;
  onNext: () => void;
  onBack: () => void;
}

export function RepoStep({ teamId, selectedRepoId, onSelect, onNext, onBack }: RepoStepProps): React.ReactElement {
  const api = useApi();
  const [mode, setMode] = useState<"select" | "create">("select");
  const [name, setName] = useState("Team Configuration");
  const [repoPath, setRepoPath] = useState(`team-config-${Math.random().toString(36).slice(2, 8)}`);
  const [branch, setBranch] = useState("main");
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);

  const reposQuery = useQuery({
    queryKey: ["setup", "repos"],
    queryFn: async () => {
      const envelope = await api.paginated<Repo[]>("/api/repos?page=1&limit=500");
      return envelope.data;
    },
  });

  const repos = reposQuery.data ?? [];
  const teamRepos = repos.filter((repo) => repo.team_id === teamId);
  const selectedRepoMissingFromList =
    Boolean(selectedRepoId) && !teamRepos.some((repo) => repo.id === selectedRepoId);

  const handleCreate = async (e: FormEvent) => {
    e.preventDefault();
    setError(null);
    if (!teamId) {
      setError("Select a team before creating a repository.");
      return;
    }
    setLoading(true);
    try {
      const created = await api.data<Repo>(`/api/teams/${teamId}/repos`, {
        method: "POST",
        body: JSON.stringify({
          name,
          repo_path: repoPath,
          integration_branch: branch,
        }),
      });
      onSelect(created.id);
      await reposQuery.refetch();
      onNext();
    } catch (cause) {
      setError(cause instanceof ApiError ? cause.message : "Failed to create repository");
    } finally {
      setLoading(false);
    }
  };

  const handleSelectAndContinue = () => {
    if (selectedRepoId) {
      onNext();
    }
  };

  return (
    <div className="space-y-4">
      <div>
        <h2 className="text-lg font-semibold font-heading">Repository</h2>
        <p className="text-sm text-muted-foreground">Select an existing repository or create a new one.</p>
      </div>

      {error && (
        <div className="rounded-md bg-destructive/10 border border-destructive/30 px-3 py-2 text-sm text-destructive">
          {error}
        </div>
      )}
      {reposQuery.isError && (
        <div className="rounded-md border border-destructive/30 bg-destructive/10 px-3 py-2 text-sm text-destructive">
          Failed to load repositories.
        </div>
      )}

      <div className="flex gap-2">
        <Button
          variant={mode === "select" ? "primary" : "secondary"}
          size="sm"
          onClick={() => setMode("select")}
          type="button"
        >
          Select existing
        </Button>
        <Button
          variant={mode === "create" ? "primary" : "secondary"}
          size="sm"
          onClick={() => setMode("create")}
          type="button"
        >
          Create new
        </Button>
      </div>

      {mode === "select" ? (
        <Card className="space-y-3">
          <Select
            value={selectedRepoId}
            onChange={(e) => onSelect(e.target.value)}
            disabled={reposQuery.isLoading}
          >
            <option value="">Select a repository...</option>
            {reposQuery.isLoading ? (
              <option value="" disabled>
                Loading repositories...
              </option>
            ) : null}
            {!reposQuery.isLoading && teamRepos.length === 0 ? (
              <option value="" disabled>
                No repositories found for this team. Create one.
              </option>
            ) : null}
            {selectedRepoMissingFromList ? (
              <option value={selectedRepoId}>
                Selected repository ({selectedRepoId.slice(0, 8)})
              </option>
            ) : null}
            {teamRepos.map((r) => (
              <option key={r.id} value={r.id}>{r.name}</option>
            ))}
          </Select>
          <div className="flex justify-between">
            <Button variant="ghost" onClick={onBack} type="button">Back</Button>
            <Button
              onClick={handleSelectAndContinue}
              disabled={!selectedRepoId || reposQuery.isLoading}
              type="button"
            >
              Continue
            </Button>
          </div>
        </Card>
      ) : (
        <Card>
          <form className="space-y-3" onSubmit={(e) => void handleCreate(e)}>
            <Input label="Repository name" id="repo-name" value={name} onChange={(e) => setName(e.target.value)} required />
            <Input label="Path" id="repo-path" value={repoPath} onChange={(e) => setRepoPath(e.target.value)} required />
            <Input label="Integration branch" id="repo-branch" value={branch} onChange={(e) => setBranch(e.target.value)} required />
            <div className="flex justify-between">
              <Button variant="ghost" onClick={onBack} type="button">Back</Button>
              <Button type="submit" disabled={loading || !teamId}>
                {loading ? "Creating..." : "Create & continue"}
              </Button>
            </div>
          </form>
        </Card>
      )}
    </div>
  );
}

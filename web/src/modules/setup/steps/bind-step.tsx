import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { Select } from "@/components/ui/select";
import { useApi } from "@/hooks/use-api";
import type { Repo } from "@/types/api";
import { ApiError } from "@/api/client";

interface BindStepProps {
  selectedRepoId: string;
  onSelect: (repoId: string) => void;
  onBind: () => void;
  onBack: () => void;
}

export function BindStep({ selectedRepoId, onSelect, onBind, onBack }: BindStepProps): React.ReactElement {
  const api = useApi();
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
  const selectedRepoMissingFromList =
    Boolean(selectedRepoId) && !repos.some((repo) => repo.id === selectedRepoId);

  const handleBind = async () => {
    if (!selectedRepoId) return;
    setError(null);
    setLoading(true);
    try {
      await api.data("/api/repo", {
        method: "PATCH",
        body: JSON.stringify({ repo_id: selectedRepoId }),
      });
      onBind();
    } catch (cause) {
      setError(cause instanceof ApiError ? cause.message : "Failed to bind repository");
    } finally {
      setLoading(false);
    }
  };

  return (
    <div className="space-y-4">
      <div>
        <h2 className="text-lg font-semibold font-heading">Bind Repository</h2>
        <p className="text-sm text-muted-foreground">
          Connect this Conman instance to a repository.
        </p>
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

      <Card className="space-y-3">
        <Select
          value={selectedRepoId}
          onChange={(e) => onSelect(e.target.value)}
          disabled={reposQuery.isLoading}
        >
          <option value="">Select repository to bind...</option>
          {reposQuery.isLoading ? (
            <option value="" disabled>
              Loading repositories...
            </option>
          ) : null}
          {!reposQuery.isLoading && repos.length === 0 ? (
            <option value="" disabled>
              No repositories available to bind.
            </option>
          ) : null}
          {selectedRepoMissingFromList ? (
            <option value={selectedRepoId}>
              Selected repository ({selectedRepoId.slice(0, 8)})
            </option>
          ) : null}
          {repos.map((r) => (
            <option key={r.id} value={r.id}>{r.name} ({r.id.slice(0, 8)})</option>
          ))}
        </Select>
        <div className="flex justify-between">
          <Button variant="ghost" onClick={onBack} type="button">Back</Button>
          <Button
            onClick={() => void handleBind()}
            disabled={!selectedRepoId || loading || reposQuery.isLoading}
            type="button"
          >
            {loading ? "Binding..." : "Bind repository"}
          </Button>
        </div>
      </Card>
    </div>
  );
}

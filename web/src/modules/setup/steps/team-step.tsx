import { FormEvent, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Select } from "@/components/ui/select";
import { useApi } from "@/hooks/use-api";
import type { Team } from "@/types/api";
import { ApiError } from "@/api/client";

interface TeamStepProps {
  selectedTeamId: string;
  onSelect: (teamId: string) => void;
  onNext: () => void;
  onBack: () => void;
}

export function TeamStep({ selectedTeamId, onSelect, onNext, onBack }: TeamStepProps): React.ReactElement {
  const api = useApi();
  const [mode, setMode] = useState<"select" | "create">("select");
  const [name, setName] = useState("Conman Team");
  const [slug, setSlug] = useState(`team-${Math.random().toString(36).slice(2, 8)}`);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);

  const teamsQuery = useQuery({
    queryKey: ["setup", "teams"],
    queryFn: async () => {
      const envelope = await api.paginated<Team[]>("/api/teams?page=1&limit=500");
      return envelope.data;
    },
  });

  const teams = teamsQuery.data ?? [];

  const handleCreate = async (e: FormEvent) => {
    e.preventDefault();
    setError(null);
    setLoading(true);
    try {
      const created = await api.data<Team>("/api/teams", {
        method: "POST",
        body: JSON.stringify({ name, slug }),
      });
      onSelect(created.id);
      await teamsQuery.refetch();
      onNext();
    } catch (cause) {
      setError(cause instanceof ApiError ? cause.message : "Failed to create team");
    } finally {
      setLoading(false);
    }
  };

  const handleSelectAndContinue = () => {
    if (selectedTeamId) {
      onNext();
    }
  };

  return (
    <div className="space-y-4">
      <div>
        <h2 className="text-lg font-semibold font-heading">Team</h2>
        <p className="text-sm text-muted-foreground">Select an existing team or create a new one.</p>
      </div>

      {error && (
        <div className="rounded-md bg-destructive/10 border border-destructive/30 px-3 py-2 text-sm text-destructive">
          {error}
        </div>
      )}
      {teamsQuery.isError && (
        <div className="rounded-md border border-destructive/30 bg-destructive/10 px-3 py-2 text-sm text-destructive">
          Failed to load teams.
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
            value={selectedTeamId}
            onChange={(e) => onSelect(e.target.value)}
            disabled={teamsQuery.isLoading}
          >
            <option value="">Select a team...</option>
            {teamsQuery.isLoading ? (
              <option value="" disabled>
                Loading teams...
              </option>
            ) : null}
            {!teamsQuery.isLoading && teams.length === 0 ? (
              <option value="" disabled>
                No teams found. Create a new team.
              </option>
            ) : null}
            {teams.map((t) => (
              <option key={t.id} value={t.id}>{t.name}</option>
            ))}
          </Select>
          <div className="flex justify-between">
            <Button variant="ghost" onClick={onBack} type="button">Back</Button>
            <Button
              onClick={handleSelectAndContinue}
              disabled={!selectedTeamId || teamsQuery.isLoading}
              type="button"
            >
              Continue
            </Button>
          </div>
        </Card>
      ) : (
        <Card>
          <form className="space-y-3" onSubmit={(e) => void handleCreate(e)}>
            <Input label="Team name" id="team-name" value={name} onChange={(e) => setName(e.target.value)} required />
            <Input label="Slug" id="team-slug" value={slug} onChange={(e) => setSlug(e.target.value)} required />
            <div className="flex justify-between">
              <Button variant="ghost" onClick={onBack} type="button">Back</Button>
              <Button type="submit" disabled={loading}>
                {loading ? "Creating..." : "Create & continue"}
              </Button>
            </div>
          </form>
        </Card>
      )}
    </div>
  );
}

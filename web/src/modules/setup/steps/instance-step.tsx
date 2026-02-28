import { FormEvent, useEffect, useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";

import { ApiError } from "@/api/client";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Select } from "@/components/ui/select";
import { useApi } from "@/hooks/use-api";
import type { Repo, Team } from "@/types/api";

interface CreateInstanceResponse {
  token: string;
  team: Team;
  repo: Repo;
  instance_slug: string;
}

interface InstanceStepProps {
  onCreated: (payload: { token: string; repoId: string; instanceSlug: string }) => void;
}

function slugify(value: string): string {
  return value
    .toLowerCase()
    .trim()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "")
    .slice(0, 63);
}

function bumpDisplayName(value: string): string {
  const trimmed = value.trim() || "My Instance";
  const match = trimmed.match(/^(.*)\s+(\d+)$/);
  if (match) {
    return `${match[1]} ${Number(match[2]) + 1}`;
  }
  return `${trimmed} 2`;
}

function bumpSlug(value: string): string {
  const normalized = slugify(value) || "instance";
  const match = normalized.match(/^(.*)-(\d+)$/);
  if (match) {
    return `${match[1]}-${Number(match[2]) + 1}`;
  }
  return `${normalized}-2`;
}

export function InstanceStep({ onCreated }: InstanceStepProps): React.ReactElement {
  const api = useApi();
  const [instanceName, setInstanceName] = useState("My Instance");
  const [instanceSlug, setInstanceSlug] = useState("my-instance");
  const [selectedTeamId, setSelectedTeamId] = useState("");
  const [slugCustomized, setSlugCustomized] = useState(false);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const teamsQuery = useQuery({
    queryKey: ["setup", "teams"],
    queryFn: async () => {
      const envelope = await api.paginated<Team[]>("/api/teams?page=1&limit=500");
      return envelope.data;
    },
  });

  const teams = teamsQuery.data ?? [];

  useEffect(() => {
    if (!slugCustomized) {
      setInstanceSlug(slugify(instanceName));
    }
  }, [instanceName, slugCustomized]);

  useEffect(() => {
    if (teams.length === 1 && !selectedTeamId) {
      setSelectedTeamId(teams[0].id);
    }
  }, [teams, selectedTeamId]);

  const appHostnamePreview = useMemo(() => {
    const slug = instanceSlug || "instance";
    return `portal--${slug}.dxflow-app.com`;
  }, [instanceSlug]);

  const handleSubmit = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (!selectedTeamId) {
      setError("Select a team before creating an instance.");
      return;
    }
    setError(null);
    setLoading(true);
    try {
      const response = await api.data<CreateInstanceResponse>("/api/onboarding/instance", {
        method: "POST",
        body: JSON.stringify({
          team_id: selectedTeamId,
          instance_name: instanceName,
          instance_slug: instanceSlug,
        }),
      });
      onCreated({
        token: response.token,
        repoId: response.repo.id,
        instanceSlug: response.instance_slug,
      });
    } catch (cause) {
      if (cause instanceof ApiError) {
        const message = cause.message.toLowerCase();
        if (message.includes("instance_name is already in use")) {
          const suggestedName = bumpDisplayName(instanceName);
          setInstanceName(suggestedName);
          if (!slugCustomized) {
            setInstanceSlug(slugify(suggestedName));
          }
          setError(`Instance name is already in use. Suggested: ${suggestedName}`);
          return;
        }
        if (message.includes("instance_slug is already in use")) {
          const suggestedSlug = bumpSlug(instanceSlug);
          setInstanceSlug(suggestedSlug);
          setSlugCustomized(true);
          setError(`Instance URL key is already in use. Suggested: ${suggestedSlug}`);
          return;
        }
        setError(cause.message);
        return;
      }
      setError("Failed to create instance");
    } finally {
      setLoading(false);
    }
  };

  return (
    <div className="space-y-4">
      <div>
        <h2 className="text-lg font-semibold font-heading">Name Your Instance</h2>
        <p className="text-sm text-muted-foreground">
          This creates your instance and connects this console to it.
        </p>
      </div>

      {error ? (
        <div className="rounded-md border border-destructive/30 bg-destructive/10 px-3 py-2 text-sm text-destructive">
          {error}
        </div>
      ) : null}
      {teamsQuery.isError ? (
        <div className="rounded-md border border-destructive/30 bg-destructive/10 px-3 py-2 text-sm text-destructive">
          Failed to load teams.
        </div>
      ) : null}

      <Card>
        <form className="space-y-3" onSubmit={(event) => void handleSubmit(event)}>
          {teams.length > 1 ? (
            <Select
              value={selectedTeamId}
              onChange={(event) => setSelectedTeamId(event.target.value)}
              disabled={teamsQuery.isLoading}
            >
              <option value="">Select team...</option>
              {teams.map((team) => (
                <option key={team.id} value={team.id}>
                  {team.name}
                </option>
              ))}
            </Select>
          ) : null}
          <Input
            label="Instance name"
            id="instance-name"
            value={instanceName}
            onChange={(event) => setInstanceName(event.target.value)}
            required
          />
          <Input
            label="Instance URL key"
            id="instance-slug"
            value={instanceSlug}
            onChange={(event) => {
              setSlugCustomized(true);
              setInstanceSlug(slugify(event.target.value));
            }}
            required
          />
          <p className="text-xs text-muted-foreground">
            App URLs will look like <span className="font-medium text-foreground">{appHostnamePreview}</span>
          </p>
          <Button type="submit" disabled={loading || !selectedTeamId || !instanceSlug}>
            {loading ? "Creating instance..." : "Create instance"}
          </Button>
        </form>
      </Card>
    </div>
  );
}

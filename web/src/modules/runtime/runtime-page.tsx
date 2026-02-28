import { useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";

import { ApiError } from "@/api/client";
import { Button } from "@/components/ui/button";
import { Card, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import { useApi } from "@/hooks/use-api";
import { useRepoContext } from "@/hooks/use-repo-context";
import { JsonView } from "@/components/ui/json-view";
import { Page } from "@/modules/shared/page";

const defaultRuntimeProfile = {
  name: "Development",
  kind: "persistent_env",
  base_url: "https://dev.example.test",
  app_endpoints: {},
  env_vars: {},
  secrets: {},
  database_engine: "mongodb",
  connection_ref: "mongodb://localhost:27017/conman_dev",
  provisioning_mode: "managed",
  migration_paths: ["migrations"],
  migration_command: "echo migrate",
};

export function RuntimePage(): React.ReactElement {
  const api = useApi();
  const queryClient = useQueryClient();
  const context = useRepoContext();
  const repoId = context?.repo?.id;

  const [createBody, setCreateBody] = useState(JSON.stringify(defaultRuntimeProfile, null, 2));
  const [profileId, setProfileId] = useState("");
  const [updateBody, setUpdateBody] = useState("{}");
  const [replaceEnvBody, setReplaceEnvBody] = useState(
    JSON.stringify({ environments: [{ name: "dev", position: 1, is_canonical: true }] }, null, 2),
  );
  const [secretProfileId, setSecretProfileId] = useState("");
  const [secretKey, setSecretKey] = useState("API_KEY");
  const [error, setError] = useState<string | null>(null);
  const [revealResponse, setRevealResponse] = useState<unknown>(null);

  const profilesQuery = useQuery({
    queryKey: ["runtime-profiles", repoId],
    queryFn: () => api.paginated(`/api/repos/${repoId}/runtime-profiles?page=1&limit=100`),
    enabled: Boolean(repoId),
  });

  const environmentsQuery = useQuery({
    queryKey: ["environments", repoId],
    queryFn: () => api.data(`/api/repos/${repoId}/environments`),
    enabled: Boolean(repoId),
  });

  const refresh = async (): Promise<void> => {
    await queryClient.invalidateQueries({ queryKey: ["runtime-profiles", repoId] });
    await queryClient.invalidateQueries({ queryKey: ["environments", repoId] });
  };

  const withJsonBody = async (fn: () => Promise<void>): Promise<void> => {
    setError(null);
    try {
      await fn();
      await refresh();
    } catch (cause) {
      setError(cause instanceof ApiError ? cause.message : "runtime request failed");
    }
  };

  if (!repoId) {
    return <Page title="Runtime">Bind a repo first in Setup.</Page>;
  }

  return (
    <Page title="Runtime & Environments" description="Manage runtime profiles, secret reveal, and environment mappings.">
      {error ? <Card className="border-destructive/40 bg-destructive/10 p-3 text-sm">{error}</Card> : null}

      <div className="grid gap-4 lg:grid-cols-2">
        <Card className="space-y-2">
          <CardTitle>Create Runtime Profile</CardTitle>
          <Textarea value={createBody} onChange={(event) => setCreateBody(event.target.value)} className="min-h-60 font-mono" />
          <Button
            onClick={() =>
              void withJsonBody(async () => {
                await api.data(`/api/repos/${repoId}/runtime-profiles`, {
                  method: "POST",
                  body: createBody,
                });
              })
            }
          >
            Create profile
          </Button>
        </Card>

        <Card className="space-y-2">
          <CardTitle>Update Runtime Profile</CardTitle>
          <Input value={profileId} onChange={(event) => setProfileId(event.target.value)} placeholder="profile id" />
          <Textarea value={updateBody} onChange={(event) => setUpdateBody(event.target.value)} className="min-h-48 font-mono" />
          <Button
            onClick={() =>
              void withJsonBody(async () => {
                await api.data(`/api/repos/${repoId}/runtime-profiles/${profileId}`, {
                  method: "PATCH",
                  body: updateBody,
                });
              })
            }
            disabled={!profileId}
          >
            Update profile
          </Button>
        </Card>
      </div>

      <Card className="space-y-2">
        <CardTitle>Replace Environments</CardTitle>
        <Textarea value={replaceEnvBody} onChange={(event) => setReplaceEnvBody(event.target.value)} className="min-h-40 font-mono" />
        <Button
          onClick={() =>
            void withJsonBody(async () => {
              await api.data(`/api/repos/${repoId}/environments`, {
                method: "PATCH",
                body: replaceEnvBody,
              });
            })
          }
        >
          Replace environments
        </Button>
      </Card>

      <Card className="space-y-2">
        <CardTitle>Reveal Secret</CardTitle>
        <div className="grid gap-2 lg:grid-cols-3">
          <Input value={secretProfileId} onChange={(event) => setSecretProfileId(event.target.value)} placeholder="profile id" />
          <Input value={secretKey} onChange={(event) => setSecretKey(event.target.value)} placeholder="secret key" />
          <Button
            onClick={() =>
              void withJsonBody(async () => {
                const data = await api.data(
                  `/api/repos/${repoId}/runtime-profiles/${secretProfileId}/secrets/${encodeURIComponent(secretKey)}/reveal`,
                  {
                    method: "POST",
                    body: JSON.stringify({}),
                  },
                );
                setRevealResponse(data);
              })
            }
            disabled={!secretProfileId || !secretKey}
          >
            Reveal
          </Button>
        </div>
        {revealResponse ? <JsonView value={revealResponse} /> : null}
      </Card>

      <div className="grid gap-4 lg:grid-cols-2">
        <Card>
          <CardTitle>Runtime Profiles</CardTitle>
          <div className="mt-3">
            <JsonView value={profilesQuery.data?.data ?? []} />
          </div>
        </Card>
        <Card>
          <CardTitle>Environments</CardTitle>
          <div className="mt-3">
            <JsonView value={environmentsQuery.data ?? []} />
          </div>
        </Card>
      </div>
    </Page>
  );
}

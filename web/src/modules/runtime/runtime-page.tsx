import { useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";

import { ApiError } from "@/api/client";
import { Button } from "@/components/ui/button";
import { Card, CardDescription, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { RawDataPanel } from "@/components/ui/raw-data-panel";
import { Textarea } from "@/components/ui/textarea";
import { useApi } from "@/hooks/use-api";
import { useRepoContext } from "@/hooks/use-repo-context";
import { canManageReleases, formatRoleLabel } from "@/lib/rbac";
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
  const role = context?.role;

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

  const canManage = canManageReleases(role);

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
    <Page
      title="Runtime & Environments"
      description="Define runtime profiles and environment mapping used by deployments."
    >
      {error ? <Card className="border-destructive/40 bg-destructive/10 p-3 text-sm">{error}</Card> : null}

      <Card>
        <CardTitle>Role Scope</CardTitle>
        <CardDescription>
          You are signed in as {formatRoleLabel(role)}.
          {canManage
            ? " You can modify runtime profiles and environment mappings."
            : " Runtime changes require Config Manager or above."}
        </CardDescription>
      </Card>

      <div className="grid gap-4 lg:grid-cols-2">
        <Card className="space-y-2">
          <CardTitle>Create Runtime Profile</CardTitle>
          <CardDescription>Use the JSON template to create profile defaults for environments.</CardDescription>
          <Textarea
            value={createBody}
            onChange={(event) => setCreateBody(event.target.value)}
            className="min-h-60 font-mono"
            disabled={!canManage}
          />
          <Button
            onClick={() =>
              void withJsonBody(async () => {
                await api.data(`/api/repos/${repoId}/runtime-profiles`, {
                  method: "POST",
                  body: createBody,
                });
              })
            }
            disabled={!canManage}
          >
            Create profile
          </Button>
        </Card>

        <Card className="space-y-2">
          <CardTitle>Update Runtime Profile</CardTitle>
          <Input
            value={profileId}
            onChange={(event) => setProfileId(event.target.value)}
            placeholder="profile id"
            disabled={!canManage}
          />
          <Textarea
            value={updateBody}
            onChange={(event) => setUpdateBody(event.target.value)}
            className="min-h-48 font-mono"
            disabled={!canManage}
          />
          <Button
            onClick={() =>
              void withJsonBody(async () => {
                await api.data(`/api/repos/${repoId}/runtime-profiles/${profileId}`, {
                  method: "PATCH",
                  body: updateBody,
                });
              })
            }
            disabled={!profileId || !canManage}
          >
            Update profile
          </Button>
        </Card>
      </div>

      <Card className="space-y-2">
        <CardTitle>Replace Environments</CardTitle>
        <CardDescription>Keep the environment chain explicit (for example: dev → stage → prod).</CardDescription>
        <Textarea
          value={replaceEnvBody}
          onChange={(event) => setReplaceEnvBody(event.target.value)}
          className="min-h-40 font-mono"
          disabled={!canManage}
        />
        <Button
          onClick={() =>
            void withJsonBody(async () => {
              await api.data(`/api/repos/${repoId}/environments`, {
                method: "PATCH",
                body: replaceEnvBody,
              });
            })
          }
          disabled={!canManage}
        >
          Replace environments
        </Button>
      </Card>

      <Card className="space-y-2">
        <CardTitle>Reveal Secret</CardTitle>
        <CardDescription>Audit aid for privileged operators; values are intentionally masked elsewhere.</CardDescription>
        <div className="grid gap-2 lg:grid-cols-3">
          <Input
            value={secretProfileId}
            onChange={(event) => setSecretProfileId(event.target.value)}
            placeholder="profile id"
            disabled={!canManage}
          />
          <Input
            value={secretKey}
            onChange={(event) => setSecretKey(event.target.value)}
            placeholder="secret key"
            disabled={!canManage}
          />
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
            disabled={!secretProfileId || !secretKey || !canManage}
          >
            Reveal
          </Button>
        </div>
      </Card>

      {revealResponse ? <RawDataPanel title="Revealed secret payload" value={revealResponse} /> : null}
      <RawDataPanel title="Runtime profiles payload" value={profilesQuery.data?.data ?? []} />
      <RawDataPanel title="Environments payload" value={environmentsQuery.data ?? []} />
    </Page>
  );
}

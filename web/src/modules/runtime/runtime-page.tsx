import { useEffect, useMemo, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";

import { ApiError } from "@/api/client";
import { Button } from "@/components/ui/button";
import { Card, CardDescription, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { RawDataPanel } from "@/components/ui/raw-data-panel";
import { Select } from "@/components/ui/select";
import { Textarea } from "@/components/ui/textarea";
import { useApi } from "@/hooks/use-api";
import { useRepoContext } from "@/hooks/use-repo-context";
import { canManageReleases, formatRoleLabel } from "@/lib/rbac";
import { formatDate } from "@/lib/utils";
import { Page } from "@/modules/shared/page";

interface RuntimeProfile {
  id: string;
  name: string;
  kind: string;
  base_url: string;
  connection_ref: string;
  updated_at: string;
}

interface EnvironmentItem {
  id?: string;
  name: string;
  position: number;
  is_canonical: boolean;
  profile_id?: string | null;
}

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

  const [createName, setCreateName] = useState(defaultRuntimeProfile.name);
  const [createKind, setCreateKind] = useState(defaultRuntimeProfile.kind);
  const [createBaseUrl, setCreateBaseUrl] = useState(defaultRuntimeProfile.base_url);
  const [createConnectionRef, setCreateConnectionRef] = useState(defaultRuntimeProfile.connection_ref);

  const [selectedProfileId, setSelectedProfileId] = useState("");
  const [updateName, setUpdateName] = useState("");
  const [updateBaseUrl, setUpdateBaseUrl] = useState("");
  const [updateConnectionRef, setUpdateConnectionRef] = useState("");
  const [advancedPatchBody, setAdvancedPatchBody] = useState("");

  const [environmentDrafts, setEnvironmentDrafts] = useState<EnvironmentItem[]>([]);
  const [newEnvironmentName, setNewEnvironmentName] = useState("");
  const [newEnvironmentProfileId, setNewEnvironmentProfileId] = useState("");

  const [secretProfileId, setSecretProfileId] = useState("");
  const [secretKey, setSecretKey] = useState("API_KEY");

  const [status, setStatus] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [revealResponse, setRevealResponse] = useState<unknown>(null);

  const canManage = canManageReleases(role);

  const profilesQuery = useQuery({
    queryKey: ["runtime-profiles", repoId],
    queryFn: async () => {
      const envelope = await api.paginated<RuntimeProfile[]>(`/api/repos/${repoId}/runtime-profiles?page=1&limit=100`);
      return envelope.data;
    },
    enabled: Boolean(repoId),
  });

  const environmentsQuery = useQuery({
    queryKey: ["environments", repoId],
    queryFn: () => api.data<EnvironmentItem[]>(`/api/repos/${repoId}/environments`),
    enabled: Boolean(repoId),
  });

  useEffect(() => {
    if (!selectedProfileId && profilesQuery.data?.[0]?.id) {
      setSelectedProfileId(profilesQuery.data[0].id);
    }
  }, [selectedProfileId, profilesQuery.data]);

  const selectedProfile = useMemo(
    () => profilesQuery.data?.find((profile) => profile.id === selectedProfileId) ?? null,
    [profilesQuery.data, selectedProfileId],
  );

  useEffect(() => {
    if (!selectedProfile) {
      setUpdateName("");
      setUpdateBaseUrl("");
      setUpdateConnectionRef("");
      return;
    }
    setUpdateName(selectedProfile.name || "");
    setUpdateBaseUrl(selectedProfile.base_url || "");
    setUpdateConnectionRef(selectedProfile.connection_ref || "");
  }, [selectedProfile]);

  useEffect(() => {
    if (!environmentsQuery.data) {
      return;
    }
    const normalized = [...environmentsQuery.data].sort((a, b) => a.position - b.position);
    setEnvironmentDrafts(normalized);
  }, [environmentsQuery.data]);

  const refresh = async (): Promise<void> => {
    await queryClient.invalidateQueries({ queryKey: ["runtime-profiles", repoId] });
    await queryClient.invalidateQueries({ queryKey: ["environments", repoId] });
  };

  const withAction = async (fn: () => Promise<void>, successMessage: string): Promise<void> => {
    setError(null);
    setStatus(null);
    try {
      await fn();
      await refresh();
      setStatus(successMessage);
    } catch (cause) {
      setError(cause instanceof ApiError ? cause.message : "runtime request failed");
    }
  };

  const addEnvironmentDraft = (): void => {
    const trimmed = newEnvironmentName.trim();
    if (!trimmed) {
      return;
    }
    setEnvironmentDrafts((prev) => [
      ...prev,
      {
        name: trimmed,
        position: prev.length + 1,
        is_canonical: prev.length === 0,
        profile_id: newEnvironmentProfileId || null,
      },
    ]);
    setNewEnvironmentName("");
    setNewEnvironmentProfileId("");
  };

  const moveEnvironment = (index: number, direction: -1 | 1): void => {
    const nextIndex = index + direction;
    if (nextIndex < 0 || nextIndex >= environmentDrafts.length) {
      return;
    }
    setEnvironmentDrafts((prev) => {
      const next = [...prev];
      const [item] = next.splice(index, 1);
      next.splice(nextIndex, 0, item);
      return next.map((env, pos) => ({ ...env, position: pos + 1 }));
    });
  };

  const removeEnvironment = (index: number): void => {
    setEnvironmentDrafts((prev) =>
      prev
        .filter((_, idx) => idx !== index)
        .map((env, pos) => ({ ...env, position: pos + 1, is_canonical: env.is_canonical && pos === 0 })),
    );
  };

  if (!repoId) {
    return <Page title="Runtime">Bind a repo first in Setup.</Page>;
  }

  return (
    <Page
      title="Runtime & Environments"
      description="Manage runtime profiles and environment chain used by release and deployment flows."
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
            ? " You can modify runtime profiles, environment mapping, and reveal secrets when needed."
            : " Runtime changes require Config Manager or above."}
        </CardDescription>
      </Card>

      <div className="grid gap-4 xl:grid-cols-[340px_1fr]">
        <Card className="space-y-3">
          <CardTitle>Runtime Profiles</CardTitle>
          <Select value={selectedProfileId} onChange={(event) => setSelectedProfileId(event.target.value)}>
            <option value="">Select profile</option>
            {(profilesQuery.data ?? []).map((profile) => (
              <option key={profile.id} value={profile.id}>
                {profile.name} ({profile.kind})
              </option>
            ))}
          </Select>

          <div className="max-h-[360px] space-y-2 overflow-auto pr-1">
            {(profilesQuery.data ?? []).map((profile) => (
              <button
                key={profile.id}
                type="button"
                className={`w-full rounded-md border p-3 text-left transition-colors ${
                  selectedProfileId === profile.id
                    ? "border-primary bg-primary/10"
                    : "border-border bg-muted/30 hover:bg-accent"
                }`}
                onClick={() => setSelectedProfileId(profile.id)}
              >
                <p className="text-sm font-semibold">{profile.name}</p>
                <p className="text-xs text-muted-foreground">{profile.kind}</p>
                <p className="text-xs text-muted-foreground">{profile.base_url}</p>
                <p className="text-xs text-muted-foreground">updated {formatDate(profile.updated_at)}</p>
              </button>
            ))}
            {!profilesQuery.data?.length ? <p className="text-sm text-muted-foreground">No runtime profiles yet.</p> : null}
          </div>
        </Card>

        <div className="space-y-4">
          <Card className="space-y-3">
            <CardTitle>Create Runtime Profile</CardTitle>
            <CardDescription>Create a baseline profile for an environment or temp env derivation.</CardDescription>
            <div className="grid gap-2 lg:grid-cols-2">
              <Input value={createName} onChange={(event) => setCreateName(event.target.value)} placeholder="profile name" />
              <Select value={createKind} onChange={(event) => setCreateKind(event.target.value)}>
                <option value="persistent_env">persistent_env</option>
                <option value="temp_workspace">temp_workspace</option>
                <option value="temp_changeset">temp_changeset</option>
              </Select>
              <Input value={createBaseUrl} onChange={(event) => setCreateBaseUrl(event.target.value)} placeholder="base url" />
              <Input
                value={createConnectionRef}
                onChange={(event) => setCreateConnectionRef(event.target.value)}
                placeholder="connection ref"
              />
            </div>
            <Button
              disabled={!canManage}
              onClick={() =>
                void withAction(
                  async () => {
                    await api.data(`/api/repos/${repoId}/runtime-profiles`, {
                      method: "POST",
                      body: JSON.stringify({
                        ...defaultRuntimeProfile,
                        name: createName,
                        kind: createKind,
                        base_url: createBaseUrl,
                        connection_ref: createConnectionRef,
                      }),
                    });
                  },
                  "Runtime profile created.",
                )
              }
            >
              Create profile
            </Button>
          </Card>

          <Card className="space-y-3">
            <CardTitle>Update Selected Profile</CardTitle>
            <CardDescription>Use typed fields for common edits. JSON patch remains available below.</CardDescription>
            <div className="grid gap-2 lg:grid-cols-2">
              <Input value={updateName} onChange={(event) => setUpdateName(event.target.value)} placeholder="name" />
              <Input value={updateBaseUrl} onChange={(event) => setUpdateBaseUrl(event.target.value)} placeholder="base url" />
              <Input
                className="lg:col-span-2"
                value={updateConnectionRef}
                onChange={(event) => setUpdateConnectionRef(event.target.value)}
                placeholder="connection ref"
              />
            </div>
            <div className="flex flex-wrap gap-2">
              <Button
                disabled={!selectedProfileId || !canManage}
                onClick={() =>
                  void withAction(
                    async () => {
                      await api.data(`/api/repos/${repoId}/runtime-profiles/${selectedProfileId}`, {
                        method: "PATCH",
                        body: JSON.stringify({
                          name: updateName,
                          base_url: updateBaseUrl,
                          connection_ref: updateConnectionRef,
                        }),
                      });
                    },
                    "Runtime profile updated.",
                  )
                }
              >
                Save profile
              </Button>

              <Button
                variant="outline"
                disabled={!selectedProfileId || !canManage}
                onClick={() => setSecretProfileId(selectedProfileId)}
              >
                Use for secret reveal
              </Button>
            </div>

            <details>
              <summary className="cursor-pointer text-xs text-muted-foreground">Advanced JSON patch</summary>
              <Textarea
                className="mt-2 min-h-28 font-mono"
                placeholder='{"env_vars":{"FEATURE_X":{"type":"boolean","value":true}}}'
                value={advancedPatchBody}
                onChange={(event) => setAdvancedPatchBody(event.target.value)}
                disabled={!selectedProfileId || !canManage}
              />
              <Button
                className="mt-2"
                type="button"
                variant="outline"
                disabled={!selectedProfileId || !canManage || !advancedPatchBody.trim()}
                onClick={() =>
                  void withAction(
                    async () => {
                      await api.data(`/api/repos/${repoId}/runtime-profiles/${selectedProfileId}`, {
                        method: "PATCH",
                        body: advancedPatchBody,
                      });
                    },
                    "Advanced profile patch applied.",
                  )
                }
              >
                Apply advanced patch
              </Button>
            </details>
          </Card>

          <Card className="space-y-3">
            <CardTitle>Environment Chain</CardTitle>
            <CardDescription>Define deploy order and canonical environment mapping.</CardDescription>

            <div className="grid gap-2 lg:grid-cols-[1fr_220px_auto]">
              <Input
                value={newEnvironmentName}
                onChange={(event) => setNewEnvironmentName(event.target.value)}
                placeholder="environment name"
                disabled={!canManage}
              />
              <Select
                value={newEnvironmentProfileId}
                onChange={(event) => setNewEnvironmentProfileId(event.target.value)}
                disabled={!canManage}
              >
                <option value="">profile (optional)</option>
                {(profilesQuery.data ?? []).map((profile) => (
                  <option key={profile.id} value={profile.id}>
                    {profile.name}
                  </option>
                ))}
              </Select>
              <Button type="button" variant="secondary" onClick={addEnvironmentDraft} disabled={!canManage}>
                Add
              </Button>
            </div>

            <div className="space-y-2">
              {environmentDrafts.map((environment, index) => (
                <div key={`${environment.name}-${index}`} className="rounded-md border border-border bg-muted/20 p-2">
                  <div className="flex items-center justify-between gap-2">
                    <div>
                      <p className="text-sm font-medium">{environment.position}. {environment.name}</p>
                      <p className="text-xs text-muted-foreground">
                        {environment.is_canonical ? "canonical" : "non-canonical"}
                        {environment.profile_id ? ` · profile ${environment.profile_id}` : ""}
                      </p>
                    </div>
                    <div className="flex gap-1">
                      <Button
                        type="button"
                        variant="outline"
                        size="sm"
                        onClick={() => moveEnvironment(index, -1)}
                        disabled={!canManage || index === 0}
                      >
                        Up
                      </Button>
                      <Button
                        type="button"
                        variant="outline"
                        size="sm"
                        onClick={() => moveEnvironment(index, 1)}
                        disabled={!canManage || index === environmentDrafts.length - 1}
                      >
                        Down
                      </Button>
                      <Button
                        type="button"
                        variant="danger"
                        size="sm"
                        onClick={() => removeEnvironment(index)}
                        disabled={!canManage}
                      >
                        Remove
                      </Button>
                    </div>
                  </div>
                </div>
              ))}
              {!environmentDrafts.length ? <p className="text-sm text-muted-foreground">No environments configured.</p> : null}
            </div>

            <Button
              disabled={!canManage}
              onClick={() =>
                void withAction(
                  async () => {
                    await api.data(`/api/repos/${repoId}/environments`, {
                      method: "PATCH",
                      body: JSON.stringify({
                        environments: environmentDrafts.map((environment, index) => ({
                          name: environment.name,
                          position: index + 1,
                          is_canonical: environment.is_canonical,
                          profile_id: environment.profile_id || null,
                        })),
                      }),
                    });
                  },
                  "Environment chain updated.",
                )
              }
            >
              Save environments
            </Button>
          </Card>

          <Card className="space-y-3">
            <CardTitle>Reveal Secret</CardTitle>
            <CardDescription>Admin/operator utility. Secret values remain masked in normal payloads.</CardDescription>
            <div className="grid gap-2 lg:grid-cols-3">
              <Select value={secretProfileId} onChange={(event) => setSecretProfileId(event.target.value)} disabled={!canManage}>
                <option value="">profile</option>
                {(profilesQuery.data ?? []).map((profile) => (
                  <option key={profile.id} value={profile.id}>
                    {profile.name}
                  </option>
                ))}
              </Select>
              <Input value={secretKey} onChange={(event) => setSecretKey(event.target.value)} placeholder="secret key" disabled={!canManage} />
              <Button
                disabled={!secretProfileId || !secretKey || !canManage}
                onClick={() =>
                  void withAction(
                    async () => {
                      const data = await api.data(
                        `/api/repos/${repoId}/runtime-profiles/${secretProfileId}/secrets/${encodeURIComponent(secretKey)}/reveal`,
                        {
                          method: "POST",
                          body: JSON.stringify({}),
                        },
                      );
                      setRevealResponse(data);
                    },
                    "Secret revealed.",
                  )
                }
              >
                Reveal
              </Button>
            </div>
          </Card>
        </div>
      </div>

      {revealResponse ? <RawDataPanel title="Revealed secret payload" value={revealResponse} /> : null}
      <details>
        <summary className="cursor-pointer text-xs text-muted-foreground">Advanced runtime payloads</summary>
        <div className="mt-2 space-y-2">
          <RawDataPanel title="Runtime profiles payload" value={profilesQuery.data ?? []} />
          <RawDataPanel title="Environments payload" value={environmentsQuery.data ?? []} />
        </div>
      </details>
    </Page>
  );
}

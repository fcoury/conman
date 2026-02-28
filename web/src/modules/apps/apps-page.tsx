import { FormEvent, useMemo, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";

import { ApiError } from "@/api/client";
import { Button } from "@/components/ui/button";
import { Card, CardDescription, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Select } from "@/components/ui/select";
import { RawDataPanel } from "@/components/ui/raw-data-panel";
import { useApi } from "@/hooks/use-api";
import { useRepoContext } from "@/hooks/use-repo-context";
import { canManageAdministration, formatRoleLabel } from "@/lib/rbac";
import { Page } from "@/modules/shared/page";
import type { App } from "@/types/api";

function slugify(value: string): string {
  return value
    .toLowerCase()
    .trim()
    .replace(/[^a-z0-9-_]+/g, "-")
    .replace(/^-+|-+$/g, "");
}

export function AppsPage(): React.ReactElement {
  const api = useApi();
  const queryClient = useQueryClient();
  const context = useRepoContext();
  const repoId = context?.repo?.id;
  const role = context?.role;

  const [appKey, setAppKey] = useState("portal");
  const [title, setTitle] = useState("Portal");
  const [domains, setDomains] = useState("");
  const [selectedAppId, setSelectedAppId] = useState("");
  const [updateTitle, setUpdateTitle] = useState("");
  const [error, setError] = useState<string | null>(null);

  const canManageApps = canManageAdministration(role);

  const appsQuery = useQuery({
    queryKey: ["apps", repoId],
    queryFn: () => api.data<App[]>(`/api/repos/${repoId}/apps`),
    enabled: Boolean(repoId),
  });

  const selectedApp = useMemo(
    () => appsQuery.data?.find((app) => app.id === selectedAppId) ?? null,
    [appsQuery.data, selectedAppId],
  );

  const previewDomain = useMemo(() => {
    const key = slugify(appKey) || "app";
    const instance = context?.repo?.repo_path || "instance";
    return `${key}--${instance}.dxflow-app.com`;
  }, [appKey, context?.repo?.repo_path]);

  const refresh = async (): Promise<void> => {
    await queryClient.invalidateQueries({ queryKey: ["apps", repoId] });
    await queryClient.invalidateQueries({ queryKey: ["repo-context"] });
  };

  const createApp = async (event: FormEvent<HTMLFormElement>): Promise<void> => {
    event.preventDefault();
    if (!repoId || !canManageApps) return;
    setError(null);
    const normalizedKey = slugify(appKey);
    if (!normalizedKey) {
      setError("App key is required.");
      return;
    }

    try {
      const customDomains = domains
        .split(",")
        .map((domain) => domain.trim())
        .filter(Boolean);
      await api.data(`/api/repos/${repoId}/apps`, {
        method: "POST",
        body: JSON.stringify({
          key: normalizedKey,
          title,
          domains: customDomains.length ? customDomains : [previewDomain],
        }),
      });
      setDomains("");
      await refresh();
    } catch (cause) {
      setError(cause instanceof ApiError ? cause.message : "failed to create app");
    }
  };

  const updateApp = async (event: FormEvent<HTMLFormElement>): Promise<void> => {
    event.preventDefault();
    if (!repoId || !selectedAppId || !canManageApps) return;
    setError(null);
    try {
      await api.data(`/api/repos/${repoId}/apps/${selectedAppId}`, {
        method: "PATCH",
        body: JSON.stringify({ title: updateTitle || selectedApp?.title }),
      });
      setUpdateTitle("");
      await refresh();
    } catch (cause) {
      setError(cause instanceof ApiError ? cause.message : "failed to update app");
    }
  };

  if (!repoId) {
    return <Page title="Apps">Bind a repo first in Setup.</Page>;
  }

  return (
    <Page
      title="Apps"
      description="Define app surfaces and routing domains. Most users only need this once during setup."
    >
      {error ? <Card className="border-destructive/40 bg-destructive/10 p-3 text-sm">{error}</Card> : null}

      {!canManageApps ? (
        <Card>
          <CardTitle>Read-only for your role</CardTitle>
          <CardDescription>
            You are signed in as {formatRoleLabel(role)}. App management is available to Admin and Owner roles.
          </CardDescription>
        </Card>
      ) : null}

      {canManageApps ? (
        <div className="grid gap-4 lg:grid-cols-2">
          <Card>
            <CardTitle>Create App</CardTitle>
            <CardDescription>New apps are typically mapped to {previewDomain}.</CardDescription>
            <form className="mt-3 space-y-2" onSubmit={(event) => void createApp(event)}>
              <Input value={appKey} onChange={(event) => setAppKey(event.target.value)} placeholder="app key" required />
              <Input value={title} onChange={(event) => setTitle(event.target.value)} placeholder="app title" required />
              <Input
                value={domains}
                onChange={(event) => setDomains(event.target.value)}
                placeholder="optional custom domains (comma-separated)"
              />
              <p className="text-xs text-muted-foreground">Preview domain: {previewDomain}</p>
              <Button type="submit">Create app</Button>
            </form>
          </Card>

          <Card>
            <CardTitle>Rename App</CardTitle>
            <CardDescription>Update display titles without changing app key or domains.</CardDescription>
            <form className="mt-3 space-y-2" onSubmit={(event) => void updateApp(event)}>
              <Select value={selectedAppId} onChange={(event) => setSelectedAppId(event.target.value)}>
                <option value="">Select app...</option>
                {appsQuery.data?.map((app) => (
                  <option key={app.id} value={app.id}>
                    {app.key} ({app.title})
                  </option>
                ))}
              </Select>
              <Input value={updateTitle} onChange={(event) => setUpdateTitle(event.target.value)} placeholder="new title" />
              <Button type="submit" disabled={!selectedAppId}>
                Update app
              </Button>
            </form>
          </Card>
        </div>
      ) : null}

      <Card>
        <CardTitle>Current Apps</CardTitle>
        <div className="mt-3 space-y-2">
          {(appsQuery.data ?? []).map((app) => (
            <div key={app.id} className="rounded-md border border-border bg-muted/30 p-3">
              <p className="text-sm font-semibold">{app.title}</p>
              <p className="text-xs text-muted-foreground">key: {app.key}</p>
              <p className="mt-1 text-xs text-muted-foreground">
                {(app.domains ?? []).length ? app.domains.join(", ") : "No domains configured"}
              </p>
            </div>
          ))}
          {!appsQuery.data?.length ? <p className="text-sm text-muted-foreground">No apps created yet.</p> : null}
        </div>
      </Card>

      <RawDataPanel title="Advanced app payload" value={appsQuery.data ?? []} />
    </Page>
  );
}

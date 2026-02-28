import { FormEvent, useMemo, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";

import { ApiError } from "@/api/client";
import { Button } from "@/components/ui/button";
import { Card, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Select } from "@/components/ui/select";
import { useApi } from "@/hooks/use-api";
import { useRepoContext } from "@/hooks/use-repo-context";
import { JsonView } from "@/components/ui/json-view";
import { Page } from "@/modules/shared/page";
import type { App } from "@/types/api";

export function AppsPage(): React.ReactElement {
  const api = useApi();
  const queryClient = useQueryClient();
  const context = useRepoContext();
  const repoId = context?.repo?.id;

  const [appKey, setAppKey] = useState("portal");
  const [title, setTitle] = useState("Portal");
  const [domains, setDomains] = useState("portal.example.test");
  const [selectedAppId, setSelectedAppId] = useState("");
  const [updateTitle, setUpdateTitle] = useState("");
  const [error, setError] = useState<string | null>(null);

  const appsQuery = useQuery({
    queryKey: ["apps", repoId],
    queryFn: () => api.data<App[]>(`/api/repos/${repoId}/apps`),
    enabled: Boolean(repoId),
  });

  const selectedApp = useMemo(
    () => appsQuery.data?.find((app) => app.id === selectedAppId) ?? null,
    [appsQuery.data, selectedAppId],
  );

  const refresh = async (): Promise<void> => {
    await queryClient.invalidateQueries({ queryKey: ["apps", repoId] });
    await queryClient.invalidateQueries({ queryKey: ["repo-context"] });
  };

  const createApp = async (event: FormEvent<HTMLFormElement>): Promise<void> => {
    event.preventDefault();
    if (!repoId) return;
    setError(null);
    try {
      await api.data(`/api/repos/${repoId}/apps`, {
        method: "POST",
        body: JSON.stringify({
          key: appKey,
          title,
          domains: domains
            .split(",")
            .map((domain) => domain.trim())
            .filter(Boolean),
        }),
      });
      await refresh();
    } catch (cause) {
      setError(cause instanceof ApiError ? cause.message : "failed to create app");
    }
  };

  const updateApp = async (event: FormEvent<HTMLFormElement>): Promise<void> => {
    event.preventDefault();
    if (!repoId || !selectedAppId) return;
    setError(null);
    try {
      await api.data(`/api/repos/${repoId}/apps/${selectedAppId}`, {
        method: "PATCH",
        body: JSON.stringify({ title: updateTitle || selectedApp?.title }),
      });
      await refresh();
    } catch (cause) {
      setError(cause instanceof ApiError ? cause.message : "failed to update app");
    }
  };

  if (!repoId) {
    return <Page title="Apps">Bind a repo first in Setup.</Page>;
  }

  return (
    <Page title="Apps" description="Manage app surfaces under the bound repository.">
      {error ? <Card className="border-destructive/40 bg-destructive/10 p-3 text-sm">{error}</Card> : null}

      <div className="grid gap-4 lg:grid-cols-2">
        <Card>
          <CardTitle>Create App</CardTitle>
          <form className="mt-3 space-y-2" onSubmit={(event) => void createApp(event)}>
            <Input value={appKey} onChange={(event) => setAppKey(event.target.value)} placeholder="app key" required />
            <Input value={title} onChange={(event) => setTitle(event.target.value)} placeholder="app title" required />
            <Input value={domains} onChange={(event) => setDomains(event.target.value)} placeholder="comma-separated domains" />
            <Button type="submit">Create app</Button>
          </form>
        </Card>

        <Card>
          <CardTitle>Update App</CardTitle>
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

      <Card>
        <CardTitle>Current Apps</CardTitle>
        <div className="mt-3">
          <JsonView value={appsQuery.data ?? []} />
        </div>
      </Card>
    </Page>
  );
}

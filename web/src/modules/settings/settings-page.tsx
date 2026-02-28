import { FormEvent, useState } from "react";
import { useQueryClient } from "@tanstack/react-query";

import { ApiError } from "@/api/client";
import { Button } from "@/components/ui/button";
import { Card, CardDescription, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { useApi } from "@/hooks/use-api";
import { useRepoContext } from "@/hooks/use-repo-context";
import { Page } from "@/modules/shared/page";

export function SettingsPage(): React.ReactElement {
  const api = useApi();
  const queryClient = useQueryClient();
  const context = useRepoContext();

  const [repoId, setRepoId] = useState(context?.repo?.id ?? "");
  const [status, setStatus] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const onSubmit = async (event: FormEvent<HTMLFormElement>): Promise<void> => {
    event.preventDefault();
    setStatus(null);
    setError(null);
    try {
      await api.data("/api/repo", {
        method: "PATCH",
        body: JSON.stringify({ repo_id: repoId }),
      });
      await queryClient.invalidateQueries({ queryKey: ["repo-context"] });
      setStatus("Bound instance updated");
    } catch (cause) {
      setError(cause instanceof ApiError ? cause.message : "failed to update bound instance");
    }
  };

  return (
    <Page title="Settings" description="Rebind this console to a different instance when required.">
      <Card>
        <CardTitle>Current Instance</CardTitle>
        <CardDescription>
          {context?.repo ? `${context.repo.name} (${context.repo.id})` : "No instance currently bound"}
        </CardDescription>
      </Card>

      <Card>
        <CardTitle>Rebind Instance</CardTitle>
        <form className="mt-3 space-y-3" onSubmit={(event) => void onSubmit(event)}>
          <Input value={repoId} onChange={(event) => setRepoId(event.target.value)} placeholder="instance id" required />
          <Button type="submit" disabled={!context?.can_rebind}>
            Apply instance binding
          </Button>
        </form>
      </Card>

      {status ? <Card className="border-success/40 bg-success/40 p-3 text-sm">{status}</Card> : null}
      {error ? <Card className="border-destructive/40 bg-destructive/10 p-3 text-sm">{error}</Card> : null}
    </Page>
  );
}

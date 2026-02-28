import { useEffect, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";

import { ApiError } from "@/api/client";
import { Button } from "@/components/ui/button";
import { Card, CardDescription, CardTitle } from "@/components/ui/card";
import { Select } from "@/components/ui/select";
import { useApi } from "@/hooks/use-api";
import { useRepoContext } from "@/hooks/use-repo-context";
import { formatRoleLabel } from "@/lib/rbac";
import { Page } from "@/modules/shared/page";
import type { NotificationPreference } from "@/types/api";

export function NotificationsPage(): React.ReactElement {
  const api = useApi();
  const queryClient = useQueryClient();
  const context = useRepoContext();
  const [emailEnabled, setEmailEnabled] = useState("true");
  const [error, setError] = useState<string | null>(null);
  const [saved, setSaved] = useState<string | null>(null);

  const preferenceQuery = useQuery({
    queryKey: ["notification-preferences"],
    queryFn: () => api.data<NotificationPreference>("/api/me/notification-preferences"),
  });

  useEffect(() => {
    if (preferenceQuery.data) {
      setEmailEnabled(String(preferenceQuery.data.email_enabled));
    }
  }, [preferenceQuery.data]);

  const save = async (): Promise<void> => {
    setError(null);
    setSaved(null);
    try {
      await api.data("/api/me/notification-preferences", {
        method: "PATCH",
        body: JSON.stringify({ email_enabled: emailEnabled === "true" }),
      });
      await queryClient.invalidateQueries({ queryKey: ["notification-preferences"] });
      setSaved("Notification preference saved.");
    } catch (cause) {
      setError(cause instanceof ApiError ? cause.message : "failed to update preferences");
    }
  };

  return (
    <Page title="Notification Preferences" description="Control user-level email notification delivery.">
      <Card>
        <CardTitle>Role Scope</CardTitle>
        <CardDescription>
          You are signed in as {formatRoleLabel(context?.role)}. Notification preferences are user-level and available for all roles.
        </CardDescription>
      </Card>
      {error ? <Card className="border-destructive/40 bg-destructive/10 p-3 text-sm">{error}</Card> : null}
      {saved ? (
        <Card className="border-success/40 bg-success/40 p-3 text-sm" aria-live="polite">
          {saved}
        </Card>
      ) : null}
      <Card className="space-y-3">
        <CardTitle>Email Notifications</CardTitle>
        <CardDescription>Toggle email fanout for review, release, and deployment events.</CardDescription>
        <Select
          id="notification-email-enabled"
          label="Email delivery"
          value={emailEnabled}
          onChange={(event) => setEmailEnabled(event.target.value)}
        >
          <option value="true">Enabled</option>
          <option value="false">Disabled</option>
        </Select>
        <Button onClick={() => void save()}>Save preference</Button>
      </Card>
    </Page>
  );
}

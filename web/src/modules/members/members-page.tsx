import { FormEvent, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";

import { ApiError } from "@/api/client";
import { Button } from "@/components/ui/button";
import { Card, CardDescription, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { RawDataPanel } from "@/components/ui/raw-data-panel";
import { Select } from "@/components/ui/select";
import { useApi } from "@/hooks/use-api";
import { useRepoContext } from "@/hooks/use-repo-context";
import { canManageAdministration, formatRoleLabel } from "@/lib/rbac";
import { Page } from "@/modules/shared/page";
import type { Invite, Role } from "@/types/api";

const roles: Role[] = ["member", "reviewer", "config_manager", "admin", "owner"];

export function MembersPage(): React.ReactElement {
  const api = useApi();
  const queryClient = useQueryClient();
  const context = useRepoContext();
  const repoId = context?.repo?.id;
  const role = context?.role;
  const teamId = context?.team?.id ?? null;

  const [assignUserId, setAssignUserId] = useState("");
  const [assignRole, setAssignRole] = useState<Role>("member");
  const [inviteEmail, setInviteEmail] = useState("reviewer@example.com");
  const [inviteRole, setInviteRole] = useState<Role>("reviewer");
  const [error, setError] = useState<string | null>(null);

  const canManage = canManageAdministration(role);

  const membersQuery = useQuery({
    queryKey: ["members", repoId],
    queryFn: () => api.paginated(`/api/repos/${repoId}/members?page=1&limit=100`),
    enabled: Boolean(repoId),
  });

  const invitesQuery = useQuery({
    queryKey: ["invites", teamId],
    queryFn: () => api.paginated<Invite[]>(`/api/teams/${teamId}/invites?page=1&limit=100`),
    enabled: Boolean(teamId),
  });

  const refresh = async (): Promise<void> => {
    await queryClient.invalidateQueries({ queryKey: ["members", repoId] });
    await queryClient.invalidateQueries({ queryKey: ["invites", teamId] });
  };

  const assignMember = async (event: FormEvent<HTMLFormElement>): Promise<void> => {
    event.preventDefault();
    if (!repoId || !canManage) return;
    setError(null);
    try {
      await api.data(`/api/repos/${repoId}/members`, {
        method: "POST",
        body: JSON.stringify({ user_id: assignUserId, role: assignRole }),
      });
      await refresh();
    } catch (cause) {
      setError(cause instanceof ApiError ? cause.message : "failed to assign member");
    }
  };

  const createInvite = async (event: FormEvent<HTMLFormElement>): Promise<void> => {
    event.preventDefault();
    if (!teamId || !canManage) return;
    setError(null);
    try {
      await api.data(`/api/teams/${teamId}/invites`, {
        method: "POST",
        body: JSON.stringify({ email: inviteEmail, role: inviteRole }),
      });
      await refresh();
    } catch (cause) {
      setError(cause instanceof ApiError ? cause.message : "failed to create invite");
    }
  };

  const resendInvite = async (inviteId: string): Promise<void> => {
    if (!teamId || !canManage) return;
    setError(null);
    try {
      await api.data(`/api/teams/${teamId}/invites/${inviteId}/resend`, {
        method: "POST",
        body: JSON.stringify({}),
      });
      await refresh();
    } catch (cause) {
      setError(cause instanceof ApiError ? cause.message : "failed to resend invite");
    }
  };

  const revokeInvite = async (inviteId: string): Promise<void> => {
    if (!teamId || !canManage) return;
    setError(null);
    try {
      await api.data(`/api/teams/${teamId}/invites/${inviteId}`, {
        method: "DELETE",
      });
      await refresh();
    } catch (cause) {
      setError(cause instanceof ApiError ? cause.message : "failed to revoke invite");
    }
  };

  if (!repoId) {
    return <Page title="Members">Complete instance setup first.</Page>;
  }

  return (
    <Page title="Members & Invites" description="Owners and admins manage access levels and invitation flow.">
      {error ? <Card className="border-destructive/40 bg-destructive/10 p-3 text-sm">{error}</Card> : null}

      <Card>
        <CardTitle>Role Scope</CardTitle>
        <CardDescription>
          You are signed in as {formatRoleLabel(role)}.
          {canManage ? " You can invite users and assign repository roles." : " Access management requires Admin or Owner."}
        </CardDescription>
      </Card>

      {canManage ? (
        <div className="grid gap-4 lg:grid-cols-2">
          <Card>
            <CardTitle>Assign Repository Member</CardTitle>
            <form className="mt-3 space-y-2" onSubmit={(event) => void assignMember(event)}>
              <Input value={assignUserId} onChange={(event) => setAssignUserId(event.target.value)} placeholder="user id" required />
              <Select value={assignRole} onChange={(event) => setAssignRole(event.target.value as Role)}>
                {roles.map((nextRole) => (
                  <option key={nextRole} value={nextRole}>
                    {nextRole}
                  </option>
                ))}
              </Select>
              <Button type="submit">Assign</Button>
            </form>
          </Card>

          <Card>
            <CardTitle>Create Team Invite</CardTitle>
            <form className="mt-3 space-y-2" onSubmit={(event) => void createInvite(event)}>
              <Input value={inviteEmail} onChange={(event) => setInviteEmail(event.target.value)} placeholder="email" required />
              <Select value={inviteRole} onChange={(event) => setInviteRole(event.target.value as Role)}>
                {roles.map((nextRole) => (
                  <option key={nextRole} value={nextRole}>
                    {nextRole}
                  </option>
                ))}
              </Select>
              <Button type="submit" disabled={!teamId}>
                Invite
              </Button>
            </form>
          </Card>
        </div>
      ) : null}

      <Card>
        <CardTitle>Active Invites</CardTitle>
        <div className="mt-3 space-y-2">
          {(invitesQuery.data?.data ?? []).map((invite) => (
            <div key={invite.id} className="flex items-center justify-between rounded-md border border-border bg-muted/30 p-2 text-sm">
              <div>
                <p>{invite.email}</p>
                <p className="text-xs text-muted-foreground">
                  {invite.role} · expires {new Date(invite.expires_at).toLocaleString()}
                </p>
              </div>
              {canManage ? (
                <div className="flex gap-2">
                  <Button type="button" variant="secondary" onClick={() => void resendInvite(invite.id)}>
                    Resend
                  </Button>
                  <Button type="button" variant="danger" onClick={() => void revokeInvite(invite.id)}>
                    Revoke
                  </Button>
                </div>
              ) : null}
            </div>
          ))}
          {!invitesQuery.data?.data?.length ? <p className="text-sm text-muted-foreground">No active invites.</p> : null}
        </div>
      </Card>

      <RawDataPanel title="Advanced membership payload" value={membersQuery.data?.data ?? []} />
    </Page>
  );
}

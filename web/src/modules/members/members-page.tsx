import { FormEvent, useState } from "react";
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
import type { Invite, Role } from "@/types/api";

const roles: Role[] = ["member", "reviewer", "config_manager", "admin", "owner"];

export function MembersPage(): React.ReactElement {
  const api = useApi();
  const queryClient = useQueryClient();
  const context = useRepoContext();
  const repoId = context?.repo?.id;
  const teamId = context?.team?.id ?? null;

  const [assignUserId, setAssignUserId] = useState("");
  const [assignRole, setAssignRole] = useState<Role>("member");
  const [inviteEmail, setInviteEmail] = useState("reviewer@example.com");
  const [inviteRole, setInviteRole] = useState<Role>("reviewer");
  const [error, setError] = useState<string | null>(null);

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
    if (!repoId) return;
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
    if (!teamId) return;
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
    if (!teamId) return;
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
    if (!teamId) return;
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
    <Page title="Members & Invites" description="Manage repository roles and team invitations.">
      {error ? <Card className="border-destructive/40 bg-destructive/10 p-3 text-sm">{error}</Card> : null}

      <div className="grid gap-4 lg:grid-cols-2">
        <Card>
          <CardTitle>Assign Repository Member</CardTitle>
          <form className="mt-3 space-y-2" onSubmit={(event) => void assignMember(event)}>
            <Input value={assignUserId} onChange={(event) => setAssignUserId(event.target.value)} placeholder="user id" required />
            <Select value={assignRole} onChange={(event) => setAssignRole(event.target.value as Role)}>
              {roles.map((role) => (
                <option key={role} value={role}>
                  {role}
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
              {roles.map((role) => (
                <option key={role} value={role}>
                  {role}
                </option>
              ))}
            </Select>
            <Button type="submit" disabled={!teamId}>
              Invite
            </Button>
          </form>
        </Card>
      </div>

      <Card>
        <CardTitle>Members</CardTitle>
        <div className="mt-3">
          <JsonView value={membersQuery.data?.data ?? []} />
        </div>
      </Card>

      <Card>
        <CardTitle>Active Invites</CardTitle>
        <div className="mt-3 space-y-2">
          {(invitesQuery.data?.data ?? []).map((invite) => (
            <div key={invite.id} className="bg-muted flex items-center justify-between rounded-md p-2 text-sm">
              <div>
                <p>{invite.email}</p>
                <p className="text-muted-foreground text-xs">
                  {invite.role} · expires {new Date(invite.expires_at).toLocaleString()}
                </p>
              </div>
              <div className="flex gap-2">
                <Button type="button" variant="secondary" onClick={() => void resendInvite(invite.id)}>
                  Resend
                </Button>
                <Button type="button" variant="danger" onClick={() => void revokeInvite(invite.id)}>
                  Revoke
                </Button>
              </div>
            </div>
          ))}
          {!invitesQuery.data?.data?.length ? <p className="text-muted-foreground text-sm">No active invites.</p> : null}
        </div>
      </Card>
    </Page>
  );
}

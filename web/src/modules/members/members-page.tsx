import { FormEvent, useEffect, useMemo, useState } from "react";
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
import { formatDate } from "@/lib/utils";
import { Page } from "@/modules/shared/page";
import type { Invite, Role } from "@/types/api";

interface RepoMember {
  user_id: string;
  repo_id: string;
  role: Role;
  created_at: string;
  email?: string | null;
  name?: string | null;
}

const roles: Role[] = ["member", "reviewer", "config_manager", "admin", "owner"];

export function MembersPage(): React.ReactElement {
  const api = useApi();
  const queryClient = useQueryClient();
  const context = useRepoContext();
  const repoId = context?.repo?.id;
  const role = context?.role;
  const teamId = context?.team?.id ?? null;

  const [selectedMemberUserId, setSelectedMemberUserId] = useState("");
  const [assignRole, setAssignRole] = useState<Role>("member");
  const [assignUserId, setAssignUserId] = useState("");
  const [inviteEmail, setInviteEmail] = useState("");
  const [inviteRole, setInviteRole] = useState<Role>("reviewer");
  const [status, setStatus] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const canManage = canManageAdministration(role);

  const membersQuery = useQuery({
    queryKey: ["members", repoId],
    queryFn: () => api.paginated<RepoMember[]>(`/api/repos/${repoId}/members?page=1&limit=100`),
    enabled: Boolean(repoId),
  });

  const invitesQuery = useQuery({
    queryKey: ["invites", teamId],
    queryFn: () => api.paginated<Invite[]>(`/api/teams/${teamId}/invites?page=1&limit=100`),
    enabled: Boolean(teamId),
  });

  const members = useMemo(() => membersQuery.data?.data ?? [], [membersQuery.data?.data]);

  useEffect(() => {
    if (!selectedMemberUserId && members[0]?.user_id) {
      setSelectedMemberUserId(members[0].user_id);
      setAssignRole(members[0].role);
    }
  }, [selectedMemberUserId, members]);

  const selectedMember = useMemo(
    () => members.find((member) => member.user_id === selectedMemberUserId) ?? null,
    [members, selectedMemberUserId],
  );

  useEffect(() => {
    if (selectedMember) {
      setAssignRole(selectedMember.role);
    }
  }, [selectedMember]);

  const refresh = async (): Promise<void> => {
    await queryClient.invalidateQueries({ queryKey: ["members", repoId] });
    await queryClient.invalidateQueries({ queryKey: ["invites", teamId] });
  };

  const withAction = async (fn: () => Promise<void>, successMessage: string): Promise<void> => {
    setError(null);
    setStatus(null);
    try {
      await fn();
      await refresh();
      setStatus(successMessage);
    } catch (cause) {
      setError(cause instanceof ApiError ? cause.message : "member action failed");
    }
  };

  const assignMember = async (event: FormEvent<HTMLFormElement>): Promise<void> => {
    event.preventDefault();
    if (!repoId || !canManage || !selectedMemberUserId) return;
    await withAction(
      async () => {
        await api.data(`/api/repos/${repoId}/members`, {
          method: "POST",
          body: JSON.stringify({ user_id: selectedMemberUserId, role: assignRole }),
        });
      },
      "Member role updated.",
    );
  };

  const createInvite = async (event: FormEvent<HTMLFormElement>): Promise<void> => {
    event.preventDefault();
    if (!teamId || !canManage) return;
    await withAction(
      async () => {
        await api.data(`/api/teams/${teamId}/invites`, {
          method: "POST",
          body: JSON.stringify({ email: inviteEmail, role: inviteRole }),
        });
        setInviteEmail("");
      },
      "Invite created.",
    );
  };

  const assignByUserId = async (event: FormEvent<HTMLFormElement>): Promise<void> => {
    event.preventDefault();
    if (!repoId || !canManage || !assignUserId) return;
    await withAction(
      async () => {
        await api.data(`/api/repos/${repoId}/members`, {
          method: "POST",
          body: JSON.stringify({ user_id: assignUserId, role: assignRole }),
        });
        setAssignUserId("");
      },
      "Member assignment applied.",
    );
  };

  const resendInvite = async (inviteId: string): Promise<void> => {
    if (!teamId || !canManage) return;
    await withAction(
      async () => {
        await api.data(`/api/teams/${teamId}/invites/${inviteId}/resend`, {
          method: "POST",
          body: JSON.stringify({}),
        });
      },
      "Invite resent.",
    );
  };

  const revokeInvite = async (inviteId: string): Promise<void> => {
    if (!teamId || !canManage) return;
    await withAction(
      async () => {
        await api.data(`/api/teams/${teamId}/invites/${inviteId}`, {
          method: "DELETE",
        });
      },
      "Invite revoked.",
    );
  };

  if (!repoId) {
    return <Page title="Members">Complete instance setup first.</Page>;
  }

  return (
    <Page title="Members & Invites" description="Owners and admins manage access levels and invitation flow.">
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
          {canManage ? " You can invite users and assign repository roles." : " Access management requires Admin or Owner."}
        </CardDescription>
      </Card>

      <div className="grid gap-4 xl:grid-cols-[1fr_1fr]">
        <Card className="space-y-3">
          <CardTitle>Invite User</CardTitle>
          <CardDescription>Primary access flow: invite by email and assign role.</CardDescription>
          <form className="space-y-2" onSubmit={(event) => void createInvite(event)}>
            <Input
              id="invite-email"
              label="Email"
              value={inviteEmail}
              onChange={(event) => setInviteEmail(event.target.value)}
              placeholder="user@example.com"
              required
              disabled={!canManage}
            />
            <Select
              id="invite-role"
              label="Role"
              value={inviteRole}
              onChange={(event) => setInviteRole(event.target.value as Role)}
              disabled={!canManage}
            >
              {roles.map((nextRole) => (
                <option key={nextRole} value={nextRole}>
                  {nextRole}
                </option>
              ))}
            </Select>
            <Button type="submit" disabled={!teamId || !canManage || !inviteEmail}>
              Send invite
            </Button>
          </form>
        </Card>

        <Card className="space-y-3">
          <CardTitle>Update Existing Member Role</CardTitle>
          <CardDescription>Choose an existing repo member and apply a new role.</CardDescription>
          <form className="space-y-2" onSubmit={(event) => void assignMember(event)}>
            <Select
              id="member-select"
              label="Member"
              value={selectedMemberUserId}
              onChange={(event) => setSelectedMemberUserId(event.target.value)}
              disabled={!canManage}
            >
              <option value="">Select member</option>
              {members.map((member) => (
                <option key={member.user_id} value={member.user_id}>
                  {member.name || member.email || member.user_id}
                </option>
              ))}
            </Select>
            <Select
              id="member-role"
              label="Role"
              value={assignRole}
              onChange={(event) => setAssignRole(event.target.value as Role)}
              disabled={!canManage || !selectedMemberUserId}
            >
              {roles.map((nextRole) => (
                <option key={nextRole} value={nextRole}>
                  {nextRole}
                </option>
              ))}
            </Select>
            <Button type="submit" disabled={!canManage || !selectedMemberUserId}>
              Apply role
            </Button>
          </form>

          <details>
            <summary className="cursor-pointer text-xs text-muted-foreground">Advanced: assign by user id</summary>
            <form className="mt-2 space-y-2" onSubmit={(event) => void assignByUserId(event)}>
              <Input
                id="advanced-user-id"
                label="User id"
                value={assignUserId}
                onChange={(event) => setAssignUserId(event.target.value)}
                placeholder="internal user id"
                required
                disabled={!canManage}
              />
              <Button type="submit" variant="outline" disabled={!canManage || !assignUserId}>
                Assign by id
              </Button>
            </form>
          </details>
        </Card>
      </div>

      <Card>
        <CardTitle>Current Members</CardTitle>
        <div className="mt-3 space-y-2">
          {members.map((member) => (
            <div key={member.user_id} className="rounded-md border border-border bg-muted/30 p-3 text-sm">
              <p className="font-medium">{member.name || member.email || member.user_id}</p>
              <p className="text-xs text-muted-foreground">{member.email || "No email available"}</p>
              <p className="text-xs text-muted-foreground">
                role {member.role} · joined {formatDate(member.created_at)}
              </p>
            </div>
          ))}
          {!members.length ? <p className="text-sm text-muted-foreground">No members found.</p> : null}
        </div>
      </Card>

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

      <details>
        <summary className="cursor-pointer text-xs text-muted-foreground">Advanced membership payload</summary>
        <div className="mt-2">
          <RawDataPanel title="Membership payload" value={members} />
        </div>
      </details>
    </Page>
  );
}

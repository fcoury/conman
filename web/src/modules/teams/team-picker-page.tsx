import { useState } from 'react';

import { Button } from '@/components/ui/button';
import { Card } from '@/components/ui/card';
import { Navigate, useNavigate } from 'react-router-dom';

import { useAuth } from '~/modules/auth/auth-context';

import { useTeamContext } from './team-context';

export default function TeamPickerPage() {
  const auth = useAuth();
  const teamContext = useTeamContext();
  const navigate = useNavigate();
  const [pendingTeamId, setPendingTeamId] = useState<string | null>(null);

  if (teamContext.isLoading) {
    return (
      <div className="flex min-h-screen items-center justify-center px-4">
        <p className="text-sm text-muted-foreground">Loading teams...</p>
      </div>
    );
  }

  if (teamContext.teams.length <= 1) {
    return <Navigate to="/" replace />;
  }

  async function chooseTeam(teamId: string) {
    setPendingTeamId(teamId);
    try {
      await teamContext.selectTeam(teamId);
      navigate('/', { replace: true });
    } finally {
      setPendingTeamId(null);
    }
  }

  async function signOut() {
    await auth.logout();
    navigate('/login', { replace: true });
  }

  return (
    <div className="mx-auto flex min-h-screen w-full max-w-3xl flex-col justify-center px-4 py-10">
      <Card className="border border-border bg-background shadow-sm">
        <div className="border-b border-border px-6 py-5">
          <h1 className="text-2xl font-semibold">Choose your team</h1>
          <p className="mt-1 text-sm text-muted-foreground">
            You belong to multiple teams. Pick one to continue.
          </p>
        </div>

        <div className="space-y-3 px-6 py-5">
          {teamContext.teams
            .slice()
            .sort((a, b) => a.name.localeCompare(b.name))
            .map((team) => {
              const instanceCount = teamContext.instances.filter(
                (instance) => instance.team_id === team.id,
              ).length;
              const isPending = pendingTeamId === team.id;

              return (
                <div
                  key={team.id}
                  className="flex items-center justify-between rounded-lg border border-border px-4 py-3"
                >
                  <div className="space-y-1">
                    <p className="font-medium">{team.name}</p>
                    <p className="text-xs text-muted-foreground">
                      {team.slug} · {instanceCount}{' '}
                      {instanceCount === 1 ? 'instance' : 'instances'}
                    </p>
                  </div>
                  <Button
                    size="sm"
                    onClick={() => void chooseTeam(team.id)}
                    disabled={isPending}
                  >
                    {isPending ? 'Entering...' : 'Enter team'}
                  </Button>
                </div>
              );
            })}
        </div>

        <div className="flex justify-end border-t border-border px-6 py-4">
          <Button variant="outline" size="sm" onClick={() => void signOut()}>
            Sign out
          </Button>
        </div>
      </Card>
    </div>
  );
}

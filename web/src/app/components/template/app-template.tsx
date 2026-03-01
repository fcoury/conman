import { SidebarInset, SidebarProvider, SidebarTrigger } from '@/components/ui/sidebar';
import { Button } from '@/components/ui/button';
import { Separator } from '@/components/ui/separator';
import {
  Outlet,
  useNavigate,
} from 'react-router-dom';

import { useAuth } from '~/modules/auth/auth-context';
import { useTeamContext } from '~/modules/teams/team-context';

import AppSidebar from './app-sidebar';

export default function AppTemplate() {
  const auth = useAuth();
  const teamContext = useTeamContext();
  const navigate = useNavigate();

  async function onTeamChanged(teamId: string) {
    await teamContext.selectTeam(teamId);
  }

  async function signOut() {
    await auth.logout();
    navigate('/login', { replace: true });
  }

  return (
    <SidebarProvider>
      <AppSidebar />
      <SidebarInset>
        <header className="flex h-16 shrink-0 items-center gap-2 border-b px-4">
          <SidebarTrigger className="-ml-1" />
          <Separator orientation="vertical" className="mr-2 h-4" />
          <span className="text-sm text-muted-foreground">Conman</span>
          <div className="ml-auto flex items-center gap-2">
            {teamContext.teams.length > 1 ? (
              <select
                aria-label="Switch team"
                value={teamContext.selectedTeamId ?? ''}
                onChange={(event) => {
                  const value = event.target.value;
                  if (!value) {
                    return;
                  }
                  void onTeamChanged(value);
                }}
                disabled={teamContext.isSwitchingContext}
                className="h-8 w-56 rounded-md border border-input bg-background px-2 text-sm outline-none focus:border-ring focus:ring-2 focus:ring-ring/40"
              >
                {teamContext.teams
                  .slice()
                  .sort((a, b) => a.name.localeCompare(b.name))
                  .map((team) => (
                    <option key={team.id} value={team.id}>
                      {team.name}
                    </option>
                  ))}
              </select>
            ) : null}
            <span className="hidden text-xs text-muted-foreground md:inline">
              {auth.session?.user.email}
            </span>
            <Button variant="outline" size="sm" onClick={() => void signOut()}>
              Sign out
            </Button>
          </div>
        </header>
        <main className="flex-1 p-4">
          <Outlet />
        </main>
      </SidebarInset>
    </SidebarProvider>
  );
}

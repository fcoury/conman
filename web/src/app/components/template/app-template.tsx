import { SidebarInset, SidebarProvider, SidebarTrigger } from '@/components/ui/sidebar';
import { Button } from '@/components/ui/button';
import {
  Breadcrumb,
  BreadcrumbItem,
  BreadcrumbLink,
  BreadcrumbList,
  BreadcrumbPage,
  BreadcrumbSeparator,
} from '@/components/ui/breadcrumb';
import { Link, Outlet, useLocation, useNavigate } from 'react-router-dom';

import { useAuth } from '~/modules/auth/auth-context';
import { useTeamContext } from '~/modules/teams/team-context';

import AppSidebar from './app-sidebar';

// Build breadcrumb segments from the current pathname
function useBreadcrumbs() {
  const { pathname } = useLocation();

  if (pathname === '/') {
    return [{ label: 'Dashboard', href: '/' }];
  }

  const segments = pathname.split('/').filter(Boolean);
  const crumbs: { label: string; href: string }[] = [];
  let accumulated = '';

  for (const segment of segments) {
    accumulated += `/${segment}`;
    const label = segmentLabel(segment);
    crumbs.push({ label, href: accumulated });
  }

  return crumbs;
}

function segmentLabel(segment: string): string {
  const labels: Record<string, string> = {
    workspaces: 'Workspaces',
    changesets: 'My Changes',
    instances: 'Instances',
  };
  return labels[segment] ?? segment;
}

export default function AppTemplate() {
  const auth = useAuth();
  const teamContext = useTeamContext();
  const navigate = useNavigate();
  const breadcrumbs = useBreadcrumbs();

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
        {/* Sticky header with backdrop blur */}
        <header className="sticky top-0 z-30 flex h-14 min-w-0 shrink-0 items-center gap-2 border-b bg-background/95 px-4 backdrop-blur supports-[backdrop-filter]:bg-background/60">
          <SidebarTrigger className="-ml-1" />

          <Breadcrumb className="ml-2 min-w-0 flex-1 overflow-hidden">
            <BreadcrumbList className="min-w-0 flex-nowrap overflow-hidden whitespace-nowrap">
              {breadcrumbs.map((crumb, idx) => {
                const isLast = idx === breadcrumbs.length - 1;
                return (
                  <BreadcrumbItem
                    key={crumb.href}
                    className={isLast ? 'min-w-0' : 'shrink-0'}
                  >
                    {idx > 0 && <BreadcrumbSeparator />}
                    {isLast ? (
                      <BreadcrumbPage className="block truncate">
                        {crumb.label}
                      </BreadcrumbPage>
                    ) : (
                      <BreadcrumbLink asChild className="shrink-0">
                        <Link to={crumb.href} className="block truncate">
                          {crumb.label}
                        </Link>
                      </BreadcrumbLink>
                    )}
                  </BreadcrumbItem>
                );
              })}
            </BreadcrumbList>
          </Breadcrumb>

          <div className="ml-auto flex shrink-0 items-center gap-2">
            {teamContext.teams.length > 1 ? (
              <select
                aria-label="Switch team"
                value={teamContext.selectedTeamId ?? ''}
                onChange={(event) => {
                  const value = event.target.value;
                  if (!value) return;
                  void onTeamChanged(value);
                }}
                disabled={teamContext.isSwitchingContext}
                className="h-8 w-40 rounded-md border border-input bg-background px-2 text-sm outline-none focus:border-ring focus:ring-2 focus:ring-ring/40"
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
            <span className="hidden text-xs text-muted-foreground lg:inline">
              {auth.session?.user.email}
            </span>
            <Button variant="outline" size="sm" onClick={() => void signOut()}>
              Sign out
            </Button>
          </div>
        </header>

        <Outlet />
      </SidebarInset>
    </SidebarProvider>
  );
}

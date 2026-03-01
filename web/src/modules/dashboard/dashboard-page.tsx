import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { useTeamContext } from '~/modules/teams/team-context';

export default function DashboardPage() {
  const teamContext = useTeamContext();

  return (
    <div className="flex-1 space-y-6 p-4 md:p-6">
      <h1 className="text-2xl font-bold tracking-tight">Dashboard</h1>

      <div className="grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-3">
        <Card>
          <CardHeader>
            <CardTitle className="text-sm font-medium text-muted-foreground">
              Team
            </CardTitle>
          </CardHeader>
          <CardContent>
            <p className="text-lg font-semibold">
              {teamContext.selectedTeam?.name ?? 'None selected'}
            </p>
          </CardContent>
        </Card>

        <Card>
          <CardHeader>
            <CardTitle className="text-sm font-medium text-muted-foreground">
              Active Instance
            </CardTitle>
          </CardHeader>
          <CardContent>
            <p className="text-lg font-semibold">
              {teamContext.activeInstance?.name ?? 'None'}
            </p>
          </CardContent>
        </Card>

        <Card>
          <CardHeader>
            <CardTitle className="text-sm font-medium text-muted-foreground">
              Repositories
            </CardTitle>
          </CardHeader>
          <CardContent>
            <p className="text-lg font-semibold">
              {teamContext.selectedTeamInstances.length}
            </p>
          </CardContent>
        </Card>
      </div>
    </div>
  );
}

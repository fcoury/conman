import { useTeamContext } from '~/modules/teams/team-context';

export default function DashboardPage() {
  const teamContext = useTeamContext();

  return (
    <div>
      <h1 className="text-2xl font-bold">Dashboard</h1>
      <p className="mt-2 text-muted-foreground">
        {teamContext.selectedTeam
          ? `Team: ${teamContext.selectedTeam.name}`
          : 'Welcome to Conman.'}
      </p>
      <p className="mt-1 text-sm text-muted-foreground">
        {teamContext.activeInstance
          ? `Active instance: ${teamContext.activeInstance.name}`
          : 'No active instance selected.'}
      </p>
    </div>
  );
}

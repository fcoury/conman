import { Navigate, Outlet } from 'react-router-dom';

import { useTeamContext } from './team-context';

export default function RequireTeamSelectionRoute() {
  const teamContext = useTeamContext();

  if (teamContext.isLoading) {
    return (
      <div className="flex min-h-screen items-center justify-center px-4">
        <p className="text-sm text-muted-foreground">Loading team context...</p>
      </div>
    );
  }

  if (teamContext.hasMultipleTeams && !teamContext.selectedTeamId) {
    return <Navigate to="/select-team" replace />;
  }

  return <Outlet />;
}

import { createContext, useContext, useMemo, type ReactNode } from 'react';
import { useQuery } from '@tanstack/react-query';
import { Navigate, useParams } from 'react-router-dom';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { AlertCircle } from 'lucide-react';

import { useTeamContext } from '~/modules/teams/team-context';
import { listWorkspaces } from './workspace-api';
import type { Workspace } from './workspace-types';

interface WorkspaceContextValue {
  repoId: string;
  repoName: string;
  workspace: Workspace;
}

const WorkspaceContext = createContext<WorkspaceContextValue | undefined>(undefined);

// Resolve the single repo from the team context
function useResolvedRepo() {
  const { selectedTeamInstances } = useTeamContext();

  if (selectedTeamInstances.length === 0) {
    return { repoId: null, repoName: null, error: 'No repositories found for this team' };
  }
  if (selectedTeamInstances.length > 1) {
    return { repoId: null, repoName: null, error: 'Multiple repositories not supported yet' };
  }
  const repo = selectedTeamInstances[0];
  return { repoId: repo.id, repoName: repo.name, error: null };
}

export function WorkspaceContextProvider({ children }: { children: ReactNode }) {
  const { workspaceId } = useParams<{ workspaceId: string }>();
  const { repoId, repoName, error: repoError } = useResolvedRepo();

  const workspacesQuery = useQuery({
    queryKey: ['workspaces', repoId],
    queryFn: () => listWorkspaces(repoId!),
    enabled: !!repoId,
  });

  const workspace = useMemo(() => {
    if (!workspacesQuery.data || !workspaceId) return null;
    return workspacesQuery.data.find((ws) => ws.id === workspaceId) ?? null;
  }, [workspacesQuery.data, workspaceId]);

  if (repoError) {
    return (
      <div className="flex flex-1 items-center justify-center p-6">
        <Card className="max-w-md">
          <CardHeader>
            <CardTitle className="flex items-center gap-2 text-destructive">
              <AlertCircle className="size-5" />
              Cannot open workspace
            </CardTitle>
          </CardHeader>
          <CardContent>
            <p className="text-sm text-muted-foreground">{repoError}</p>
          </CardContent>
        </Card>
      </div>
    );
  }

  if (workspacesQuery.isLoading || !repoId || !repoName) {
    return null;
  }

  if (workspacesQuery.error) {
    return (
      <div className="flex flex-1 items-center justify-center p-6">
        <Card className="max-w-md">
          <CardHeader>
            <CardTitle className="flex items-center gap-2 text-destructive">
              <AlertCircle className="size-5" />
              Failed to load workspaces
            </CardTitle>
          </CardHeader>
          <CardContent>
            <p className="text-sm text-muted-foreground">
              {workspacesQuery.error instanceof Error
                ? workspacesQuery.error.message
                : 'An unknown error occurred'}
            </p>
          </CardContent>
        </Card>
      </div>
    );
  }

  // Stale or invalid workspace URL: route back to workspace index.
  if (!workspace) {
    return <Navigate to="/workspaces" replace />;
  }

  if (!repoId || !repoName) {
    return null;
  }

  return (
    <WorkspaceContext.Provider value={{ repoId, repoName, workspace }}>
      {children}
    </WorkspaceContext.Provider>
  );
}

export function useWorkspaceContext(): WorkspaceContextValue {
  const ctx = useContext(WorkspaceContext);
  if (!ctx) {
    throw new Error('useWorkspaceContext must be used within WorkspaceContextProvider');
  }
  return ctx;
}

export { useResolvedRepo };

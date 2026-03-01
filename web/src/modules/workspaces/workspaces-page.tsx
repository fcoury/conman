import { useEffect, useRef } from 'react';
import { useNavigate } from 'react-router-dom';
import { useQuery } from '@tanstack/react-query';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Skeleton } from '@/components/ui/skeleton';
import { AlertCircle } from 'lucide-react';

import { useResolvedRepo } from './workspace-context';
import { listWorkspaces } from './workspace-api';

export default function WorkspacesPage() {
  const navigate = useNavigate();
  const { repoId, error: repoError } = useResolvedRepo();
  const didRedirect = useRef(false);

  const workspacesQuery = useQuery({
    queryKey: ['workspaces', repoId],
    queryFn: () => listWorkspaces(repoId!),
    enabled: !!repoId,
  });

  // Auto-redirect to first workspace once loaded (guard against re-fire)
  useEffect(() => {
    if (
      !didRedirect.current &&
      workspacesQuery.data &&
      workspacesQuery.data.length > 0
    ) {
      didRedirect.current = true;
      navigate(`/workspaces/${workspacesQuery.data[0].id}`, { replace: true });
    }
  }, [workspacesQuery.data, navigate]);

  // Repo resolution error
  if (repoError) {
    return (
      <div className="flex flex-1 items-center justify-center p-6">
        <Card className="max-w-md">
          <CardHeader>
            <CardTitle className="flex items-center gap-2 text-destructive">
              <AlertCircle className="size-5" />
              Cannot open workspaces
            </CardTitle>
          </CardHeader>
          <CardContent>
            <p className="text-sm text-muted-foreground">{repoError}</p>
          </CardContent>
        </Card>
      </div>
    );
  }

  // Workspace fetch error
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

  // Loading state
  return (
    <div className="flex flex-1 items-center justify-center p-6">
      <div className="flex flex-col items-center gap-4">
        <Skeleton className="h-6 w-48" />
        <Skeleton className="h-4 w-32" />
        <p className="text-sm text-muted-foreground">Loading workspaces...</p>
      </div>
    </div>
  );
}

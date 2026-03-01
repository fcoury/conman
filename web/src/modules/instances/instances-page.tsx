import { useState } from 'react';

import { Button } from '@/components/ui/button';
import { Card } from '@/components/ui/card';

import { useTeamContext } from '~/modules/teams/team-context';

export default function InstancesPage() {
  const teamContext = useTeamContext();
  const [pendingInstanceId, setPendingInstanceId] = useState<string | null>(null);

  if (!teamContext.selectedTeam) {
    return (
      <div>
        <h1 className="text-2xl font-bold">Instances</h1>
        <p className="mt-2 text-muted-foreground">
          Select a team to view its instances.
        </p>
      </div>
    );
  }

  const rows = teamContext.selectedTeamInstances
    .slice()
    .sort((a, b) => a.name.localeCompare(b.name));

  async function activateInstance(instanceId: string) {
    setPendingInstanceId(instanceId);
    try {
      await teamContext.setActiveInstance(instanceId);
    } finally {
      setPendingInstanceId(null);
    }
  }

  return (
    <div className="space-y-4">
      <div>
        <h1 className="text-2xl font-bold">Instances</h1>
        <p className="mt-2 text-muted-foreground">
          Team: <span className="font-medium text-foreground">{teamContext.selectedTeam.name}</span>
        </p>
      </div>

      {rows.length === 0 ? (
        <Card className="border border-border bg-background py-4">
          <p className="text-sm text-muted-foreground">
            This team has no instances yet.
          </p>
        </Card>
      ) : (
        <div className="space-y-3">
          {rows.map((instance) => {
            const isActive = teamContext.activeInstance?.id === instance.id;
            const isPending = pendingInstanceId === instance.id;

            return (
              <Card
                key={instance.id}
                className="border border-border bg-background py-4"
              >
                <div className="flex flex-wrap items-center justify-between gap-3">
                  <div className="space-y-1">
                    <p className="font-medium">{instance.name}</p>
                    <p className="text-xs text-muted-foreground">
                      {instance.repo_path} · {instance.integration_branch}
                    </p>
                  </div>
                  <div className="flex items-center gap-2">
                    {isActive ? (
                      <span className="rounded-md bg-primary/10 px-2 py-1 text-xs font-medium text-primary">
                        Active
                      </span>
                    ) : null}
                    <Button
                      variant={isActive ? 'outline' : 'default'}
                      size="sm"
                      disabled={isActive || isPending}
                      onClick={() => void activateInstance(instance.id)}
                    >
                      {isActive ? 'Selected' : isPending ? 'Selecting...' : 'Set active'}
                    </Button>
                  </div>
                </div>
              </Card>
            );
          })}
        </div>
      )}
    </div>
  );
}

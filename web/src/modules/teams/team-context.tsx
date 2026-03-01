import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useState,
  type ReactNode,
} from 'react';

import { useMutation, useQuery } from '@tanstack/react-query';
import { Outlet } from 'react-router-dom';

import { apiData } from '~/api/client';

import type {
  InstanceSummary,
  RepoContextResponse,
  TeamSummary,
  UpdateBoundInstanceInput,
} from './team-types';

const TEAM_KEY = 'conman.context.team_id';

interface TeamContextValue {
  teams: TeamSummary[];
  instances: InstanceSummary[];
  selectedTeamId: string | null;
  selectedTeam: TeamSummary | null;
  selectedTeamInstances: InstanceSummary[];
  activeInstance: InstanceSummary | null;
  hasMultipleTeams: boolean;
  isLoading: boolean;
  isSwitchingContext: boolean;
  selectTeam: (teamId: string) => Promise<void>;
  setActiveInstance: (instanceId: string) => Promise<void>;
  refresh: () => Promise<void>;
}

const TeamContext = createContext<TeamContextValue | undefined>(undefined);

function readSelectedTeamId(): string | null {
  return localStorage.getItem(TEAM_KEY);
}

function writeSelectedTeamId(teamId: string | null): void {
  if (teamId) {
    localStorage.setItem(TEAM_KEY, teamId);
    return;
  }
  localStorage.removeItem(TEAM_KEY);
}

function TeamContextProvider({ children }: { children: ReactNode }) {
  const [selectedTeamId, setSelectedTeamId] = useState<string | null>(() =>
    readSelectedTeamId(),
  );
  const [isEnsuringBinding, setIsEnsuringBinding] = useState(false);

  const teamsQuery = useQuery({
    queryKey: ['teams'],
    queryFn: () => apiData<TeamSummary[]>('/api/teams?page=1&limit=100'),
  });

  const instancesQuery = useQuery({
    queryKey: ['instances'],
    queryFn: () => apiData<InstanceSummary[]>('/api/repos?page=1&limit=100'),
  });

  const boundContextQuery = useQuery({
    queryKey: ['bound-instance'],
    queryFn: () => apiData<RepoContextResponse>('/api/repo'),
  });

  const bindInstanceMutation = useMutation({
    mutationFn: async ({ repo_id }: UpdateBoundInstanceInput) =>
      apiData<RepoContextResponse>('/api/repo', {
        method: 'PATCH',
        body: JSON.stringify({ repo_id }),
      }),
  });

  const teams = teamsQuery.data ?? [];
  const instances = instancesQuery.data ?? [];

  const persistSelectedTeam = useCallback((teamId: string | null) => {
    writeSelectedTeamId(teamId);
    setSelectedTeamId(teamId);
  }, []);

  const ensureBindingForTeam = useCallback(
    async (teamId: string) => {
      const currentlyBound = boundContextQuery.data?.repo;
      if (currentlyBound?.team_id === teamId) {
        return;
      }

      const teamInstances = instances
        .filter((instance) => instance.team_id === teamId)
        .sort((a, b) => a.name.localeCompare(b.name));

      if (teamInstances.length === 0) {
        return;
      }

      await bindInstanceMutation.mutateAsync({ repo_id: teamInstances[0].id });
      await boundContextQuery.refetch();
    },
    [bindInstanceMutation, boundContextQuery, instances],
  );

  useEffect(() => {
    if (!teamsQuery.data) {
      return;
    }

    if (teamsQuery.data.length === 1) {
      const singleTeamId = teamsQuery.data[0].id;
      if (selectedTeamId !== singleTeamId) {
        persistSelectedTeam(singleTeamId);
      }
      return;
    }

    const selectedExists = selectedTeamId
      ? teamsQuery.data.some((team) => team.id === selectedTeamId)
      : false;

    if (!selectedExists && selectedTeamId !== null) {
      persistSelectedTeam(null);
    }
  }, [persistSelectedTeam, selectedTeamId, teamsQuery.data]);

  useEffect(() => {
    if (!selectedTeamId || !instancesQuery.data || !boundContextQuery.data) {
      return;
    }

    let isCanceled = false;
    setIsEnsuringBinding(true);
    ensureBindingForTeam(selectedTeamId)
      .catch(() => {
        // Keep picker/app interactive even if binding fails.
      })
      .finally(() => {
        if (!isCanceled) {
          setIsEnsuringBinding(false);
        }
      });

    return () => {
      isCanceled = true;
    };
  }, [
    boundContextQuery.data,
    ensureBindingForTeam,
    instancesQuery.data,
    selectedTeamId,
  ]);

  const selectedTeam =
    teams.find((team) => team.id === selectedTeamId) ?? null;

  const selectedTeamInstances = useMemo(() => {
    if (!selectedTeamId) {
      return [];
    }
    return instances.filter((instance) => instance.team_id === selectedTeamId);
  }, [instances, selectedTeamId]);

  const activeInstance = useMemo(() => {
    const repo = boundContextQuery.data?.repo ?? null;
    if (!repo) {
      return null;
    }
    if (!selectedTeamId) {
      return repo;
    }
    return repo.team_id === selectedTeamId ? repo : null;
  }, [boundContextQuery.data, selectedTeamId]);

  const value = useMemo<TeamContextValue>(() => {
    return {
      teams,
      instances,
      selectedTeamId,
      selectedTeam,
      selectedTeamInstances,
      activeInstance,
      hasMultipleTeams: teams.length > 1,
      isLoading:
        teamsQuery.isLoading ||
        instancesQuery.isLoading ||
        boundContextQuery.isLoading,
      isSwitchingContext: bindInstanceMutation.isPending || isEnsuringBinding,
      async selectTeam(teamId) {
        persistSelectedTeam(teamId);
        await ensureBindingForTeam(teamId);
      },
      async setActiveInstance(instanceId) {
        await bindInstanceMutation.mutateAsync({ repo_id: instanceId });
        await boundContextQuery.refetch();
      },
      async refresh() {
        await Promise.all([
          teamsQuery.refetch(),
          instancesQuery.refetch(),
          boundContextQuery.refetch(),
        ]);
      },
    };
  }, [
    activeInstance,
    bindInstanceMutation,
    boundContextQuery,
    ensureBindingForTeam,
    instances,
    instancesQuery,
    isEnsuringBinding,
    persistSelectedTeam,
    selectedTeam,
    selectedTeamId,
    selectedTeamInstances,
    teams,
    teamsQuery,
  ]);

  return <TeamContext.Provider value={value}>{children}</TeamContext.Provider>;
}

export function TeamContextLayout() {
  return (
    <TeamContextProvider>
      <Outlet />
    </TeamContextProvider>
  );
}

export function useTeamContext(): TeamContextValue {
  const context = useContext(TeamContext);
  if (!context) {
    throw new Error('useTeamContext must be used within TeamContextProvider');
  }
  return context;
}

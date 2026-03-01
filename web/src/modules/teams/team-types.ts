export type UiRole =
  | 'owner'
  | 'admin'
  | 'member'
  | 'reviewer'
  | 'deployer'
  | 'auditor';

export interface TeamSummary {
  id: string;
  name: string;
  slug: string;
}

export interface InstanceSummary {
  id: string;
  team_id: string | null;
  name: string;
  repo_path: string;
  integration_branch: string;
}

export interface RepoContextResponse {
  status: 'bound' | 'unbound';
  repo: InstanceSummary | null;
  team: TeamSummary | null;
  role: UiRole | null;
}

export interface UpdateBoundInstanceInput {
  repo_id: string;
}

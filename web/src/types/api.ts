export interface PaginationMeta {
  page: number;
  limit: number;
  total: number;
}

export interface ApiResponseEnvelope<T> {
  data: T;
  pagination?: PaginationMeta;
}

export interface ApiErrorBody {
  code: string;
  message: string;
  request_id: string;
}

export interface ApiErrorEnvelope {
  error: ApiErrorBody;
}

export type Role =
  | "member"
  | "reviewer"
  | "config_manager"
  | "admin"
  | "owner";

export interface Team {
  id: string;
  name: string;
  slug: string;
}

export interface RepoSettings {
  baseline_mode: string;
  commit_mode_default: string;
  blocked_paths: string[];
  file_size_limit_bytes: number;
  profile_approval_policy: string;
}

export interface Repo {
  id: string;
  team_id?: string | null;
  name: string;
  repo_path: string;
  integration_branch: string;
  settings: RepoSettings;
  created_by: string;
  created_at: string;
  updated_at: string;
}

export interface App {
  id: string;
  repo_id: string;
  key: string;
  title: string;
  domains: string[];
  branding?: unknown;
  roles: string[];
  created_at: string;
  updated_at: string;
}

export interface UiBinding {
  id: string;
  repo_id: string;
  configured_by: string;
  configured_at: string;
  updated_at: string;
}

export interface RepoContextResponse {
  status: "bound" | "unbound";
  binding: UiBinding | null;
  repo: Repo | null;
  team: Team | null;
  apps: App[];
  role: Role | null;
  can_rebind: boolean;
}

export interface Invite {
  id: string;
  team_id: string;
  email: string;
  role: Role;
  token: string;
  invited_by: string;
  expires_at: string;
  accepted_at: string | null;
  created_at: string;
}

export interface Workspace {
  id: string;
  repo_id: string;
  owner_user_id: string;
  branch_name: string;
  title?: string | null;
  is_default: boolean;
  base_ref_type: string;
  base_ref_value: string;
  head_sha: string;
  created_at: string;
  updated_at: string;
}

export interface Changeset {
  id: string;
  repo_id: string;
  workspace_id: string;
  title: string;
  description?: string | null;
  state: string;
  author_user_id: string;
  head_sha: string;
  submitted_head_sha?: string | null;
  revision: number;
  approvals: Array<{ user_id: string; role: Role; approved_at: string }>;
  queue_position?: number | null;
  queued_at?: string | null;
  created_at: string;
  updated_at: string;
}

export interface ReleaseBatch {
  id: string;
  repo_id: string;
  tag: string;
  state: string;
  ordered_changeset_ids: string[];
  compose_job_id?: string | null;
  published_sha?: string | null;
  published_at?: string | null;
  published_by?: string | null;
  created_at: string;
  updated_at: string;
}

export interface Deployment {
  id: string;
  repo_id: string;
  environment_id: string;
  release_id: string;
  state: string;
  is_skip_stage: boolean;
  is_concurrent_batch: boolean;
  approvals: string[];
  job_id?: string | null;
  created_by: string;
  created_at: string;
  updated_at: string;
}

export interface Job {
  id: string;
  repo_id: string;
  job_type: string;
  state: string;
  entity_type: string;
  entity_id: string;
  payload: unknown;
  result?: unknown;
  error_message?: string | null;
  retry_count: number;
  max_retries: number;
  timeout_ms: number;
  created_by?: string | null;
  created_at: string;
  started_at?: string | null;
  finished_at?: string | null;
  updated_at: string;
}

export interface NotificationPreference {
  id: string;
  user_id: string;
  email_enabled: boolean;
  created_at: string;
  updated_at: string;
}

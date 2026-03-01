export type ChangesetState =
  | 'draft'
  | 'submitted'
  | 'in_review'
  | 'approved'
  | 'changes_requested'
  | 'rejected'
  | 'queued'
  | 'released'
  | 'conflicted'
  | 'needs_revalidation';

export interface Changeset {
  id: string;
  repo_id: string;
  workspace_id: string;
  title: string;
  description: string | null;
  state: ChangesetState;
  author_user_id: string;
  head_sha: string;
  submitted_head_sha: string | null;
  revision: number;
  created_at: string;
  updated_at: string;
}

export interface WorkspaceChangesEntry {
  path: string;
  old_path: string | null;
  additions: number;
  deletions: number;
}

export interface WorkspaceChangesResponse {
  workspace_id: string;
  base_sha: string;
  head_sha: string;
  has_changes: boolean;
  files_changed: number;
  additions: number;
  deletions: number;
  entries: WorkspaceChangesEntry[];
}

export interface WorkspacePatchResponse {
  workspace_id: string;
  base_sha: string;
  head_sha: string;
  path: string;
  patch: string;
  binary: boolean;
  lines_added: number;
  lines_removed: number;
}

export interface OpenWorkspaceChangesetResponse {
  changeset: Changeset | null;
}

export interface CreateChangesetInput {
  workspace_id: string;
  title: string;
  description?: string;
}

export interface UpdateChangesetInput {
  title?: string;
  description?: string;
}

import { apiData } from '~/api/client';
import type {
  Changeset,
  CreateChangesetInput,
  OpenWorkspaceChangesetResponse,
  UpdateChangesetInput,
  WorkspaceChangesResponse,
  WorkspacePatchResponse,
} from './changesets-types';

export function getWorkspaceChanges(
  repoId: string,
  workspaceId: string,
): Promise<WorkspaceChangesResponse> {
  return apiData<WorkspaceChangesResponse>(
    `/api/repos/${repoId}/workspaces/${workspaceId}/changes`,
  );
}

export function getWorkspaceChangePatch(
  repoId: string,
  workspaceId: string,
  path: string,
): Promise<WorkspacePatchResponse> {
  return apiData<WorkspacePatchResponse>(
    `/api/repos/${repoId}/workspaces/${workspaceId}/changes/patch?path=${encodeURIComponent(path)}`,
  );
}

export async function getOpenWorkspaceChangeset(
  repoId: string,
  workspaceId: string,
): Promise<Changeset | null> {
  const response = await apiData<OpenWorkspaceChangesetResponse>(
    `/api/repos/${repoId}/workspaces/${workspaceId}/open-changeset`,
  );
  return response.changeset;
}

export function createChangeset(
  repoId: string,
  input: CreateChangesetInput,
): Promise<Changeset> {
  return apiData<Changeset>(`/api/repos/${repoId}/changesets`, {
    method: 'POST',
    body: JSON.stringify(input),
  });
}

export function updateChangeset(
  repoId: string,
  changesetId: string,
  input: UpdateChangesetInput,
): Promise<Changeset> {
  return apiData<Changeset>(`/api/repos/${repoId}/changesets/${changesetId}`, {
    method: 'PATCH',
    body: JSON.stringify(input),
  });
}

export function submitChangeset(
  repoId: string,
  changesetId: string,
): Promise<unknown> {
  return apiData<unknown>(`/api/repos/${repoId}/changesets/${changesetId}/submit`, {
    method: 'POST',
    body: JSON.stringify({}),
  });
}

export function resubmitChangeset(
  repoId: string,
  changesetId: string,
): Promise<unknown> {
  return apiData<unknown>(`/api/repos/${repoId}/changesets/${changesetId}/resubmit`, {
    method: 'POST',
    body: JSON.stringify({}),
  });
}

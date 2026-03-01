import { apiData } from '~/api/client';
import type {
  FileContentResponse,
  FileTreeResponse,
  FileWriteResponse,
  Workspace,
} from './workspace-types';

export function listWorkspaces(repoId: string): Promise<Workspace[]> {
  return apiData<Workspace[]>(`/api/repos/${repoId}/workspaces`);
}

export function getFileTree(
  repoId: string,
  wsId: string,
  path = '',
  recursive = true,
): Promise<FileTreeResponse> {
  const params = new URLSearchParams();
  if (path) params.set('path', path);
  if (recursive) params.set('recursive', 'true');
  return apiData<FileTreeResponse>(
    `/api/repos/${repoId}/workspaces/${wsId}/files?${params.toString()}`,
  );
}

export function getFileContent(
  repoId: string,
  wsId: string,
  path: string,
): Promise<FileContentResponse> {
  return apiData<FileContentResponse>(
    `/api/repos/${repoId}/workspaces/${wsId}/files?path=${encodeURIComponent(path)}`,
  );
}

export function writeFile(
  repoId: string,
  wsId: string,
  path: string,
  contentBase64: string,
): Promise<FileWriteResponse> {
  return apiData<FileWriteResponse>(
    `/api/repos/${repoId}/workspaces/${wsId}/files`,
    {
      method: 'PUT',
      body: JSON.stringify({ path, content: contentBase64 }),
    },
  );
}

export function deleteFile(
  repoId: string,
  wsId: string,
  path: string,
): Promise<void> {
  return apiData<void>(
    `/api/repos/${repoId}/workspaces/${wsId}/files`,
    {
      method: 'DELETE',
      body: JSON.stringify({ path }),
    },
  );
}

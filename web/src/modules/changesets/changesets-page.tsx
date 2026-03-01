import { useEffect, useMemo, useState } from 'react';
import { useMutation, useQuery } from '@tanstack/react-query';
import { AlertCircle, FileDiff, RefreshCw } from 'lucide-react';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Input } from '@/components/ui/input';
import { ScrollArea } from '@/components/ui/scroll-area';
import { Skeleton } from '@/components/ui/skeleton';
import { Textarea } from '@/components/ui/textarea';
import { toast } from '@/lib/toast/toast';

import { ApiError } from '~/api/client';
import { listWorkspaces } from '~/modules/workspaces/workspace-api';
import { useResolvedRepo } from '~/modules/workspaces/workspace-context';
import type { Workspace } from '~/modules/workspaces/workspace-types';
import {
  createChangeset,
  getOpenWorkspaceChangeset,
  getWorkspaceChangePatch,
  getWorkspaceChanges,
  resubmitChangeset,
  submitChangeset,
  updateChangeset,
} from './changesets-api';

function workspaceStorageKey(repoId: string): string {
  return `conman.my-changes.workspace.${repoId}`;
}

function readWorkspaceSelection(repoId: string): string | null {
  if (typeof window === 'undefined') return null;
  return window.localStorage.getItem(workspaceStorageKey(repoId));
}

function writeWorkspaceSelection(repoId: string, workspaceId: string): void {
  if (typeof window === 'undefined') return;
  window.localStorage.setItem(workspaceStorageKey(repoId), workspaceId);
}

function workspaceLabel(workspace: Workspace): string {
  return workspace.title?.trim() || workspace.branch_name;
}

function shortSha(value: string): string {
  return value ? value.slice(0, 8) : '';
}

function canSubmitOrResubmit(state: string): boolean {
  return ['draft', 'submitted', 'in_review', 'changes_requested'].includes(state);
}

function isDraftState(state: string): boolean {
  return state === 'draft';
}

export default function ChangesetsPage() {
  const { repoId, repoName, error: repoError } = useResolvedRepo();
  const [selectedWorkspaceId, setSelectedWorkspaceId] = useState<string | null>(null);
  const [filter, setFilter] = useState('');
  const [selectedPath, setSelectedPath] = useState<string | null>(null);
  const [draftTitle, setDraftTitle] = useState('');
  const [draftDescription, setDraftDescription] = useState('');
  const [editTitle, setEditTitle] = useState('');
  const [editDescription, setEditDescription] = useState('');

  const workspacesQuery = useQuery({
    queryKey: ['workspaces', repoId],
    queryFn: () => listWorkspaces(repoId!),
    enabled: !!repoId,
  });

  const selectedWorkspace = useMemo(() => {
    if (!workspacesQuery.data || !selectedWorkspaceId) return null;
    return workspacesQuery.data.find((workspace) => workspace.id === selectedWorkspaceId) ?? null;
  }, [selectedWorkspaceId, workspacesQuery.data]);

  useEffect(() => {
    if (!repoId || !workspacesQuery.data || workspacesQuery.data.length === 0) {
      return;
    }

    const remembered = readWorkspaceSelection(repoId);
    const resolved =
      workspacesQuery.data.find((workspace) => workspace.id === remembered) ??
      workspacesQuery.data.find((workspace) => workspace.is_default) ??
      workspacesQuery.data[0];

    setSelectedWorkspaceId((current) => {
      if (current && workspacesQuery.data.some((workspace) => workspace.id === current)) {
        return current;
      }
      return resolved.id;
    });
  }, [repoId, workspacesQuery.data]);

  useEffect(() => {
    if (!repoId || !selectedWorkspaceId) return;
    writeWorkspaceSelection(repoId, selectedWorkspaceId);
    setSelectedPath(null);
  }, [repoId, selectedWorkspaceId]);

  const changesQuery = useQuery({
    queryKey: ['workspace-changes', repoId, selectedWorkspaceId],
    queryFn: () => getWorkspaceChanges(repoId!, selectedWorkspaceId!),
    enabled: !!repoId && !!selectedWorkspaceId,
  });

  const openChangesetQuery = useQuery({
    queryKey: ['workspace-open-changeset', repoId, selectedWorkspaceId],
    queryFn: () => getOpenWorkspaceChangeset(repoId!, selectedWorkspaceId!),
    enabled: !!repoId && !!selectedWorkspaceId,
  });

  const entries = changesQuery.data?.entries ?? [];
  const filteredEntries = useMemo(() => {
    const value = filter.trim().toLowerCase();
    if (!value) return entries;
    return entries.filter((entry) => entry.path.toLowerCase().includes(value));
  }, [entries, filter]);

  useEffect(() => {
    if (entries.length === 0) {
      setSelectedPath(null);
      return;
    }
    if (!selectedPath || !entries.some((entry) => entry.path === selectedPath)) {
      setSelectedPath(entries[0].path);
    }
  }, [entries, selectedPath]);

  const patchQuery = useQuery({
    queryKey: ['workspace-change-patch', repoId, selectedWorkspaceId, selectedPath],
    queryFn: () => getWorkspaceChangePatch(repoId!, selectedWorkspaceId!, selectedPath!),
    enabled: !!repoId && !!selectedWorkspaceId && !!selectedPath,
  });

  const openChangeset = openChangesetQuery.data;

  useEffect(() => {
    if (!openChangeset) {
      setEditTitle('');
      setEditDescription('');
      return;
    }
    setEditTitle(openChangeset.title);
    setEditDescription(openChangeset.description ?? '');
  }, [openChangeset?.id, openChangeset?.title, openChangeset?.description]);

  const createDraftMutation = useMutation({
    mutationFn: () =>
      createChangeset(repoId!, {
        workspace_id: selectedWorkspaceId!,
        title: draftTitle.trim(),
        description: draftDescription.trim() || undefined,
      }),
    onSuccess: async () => {
      setDraftTitle('');
      setDraftDescription('');
      toast({ type: 'success', title: 'Draft created' });
      await openChangesetQuery.refetch();
      await changesQuery.refetch();
    },
    onError: (error) => {
      toast({
        type: 'error',
        title: 'Failed to create draft',
        subtitle: error instanceof Error ? error.message : 'Unknown error',
      });
    },
  });

  const updateMetadataMutation = useMutation({
    mutationFn: () =>
      updateChangeset(repoId!, openChangeset!.id, {
        title: editTitle.trim(),
        description: editDescription.trim() || '',
      }),
    onSuccess: async () => {
      toast({ type: 'success', title: 'Changeset updated' });
      await openChangesetQuery.refetch();
    },
    onError: (error) => {
      toast({
        type: 'error',
        title: 'Failed to update changeset',
        subtitle: error instanceof Error ? error.message : 'Unknown error',
      });
    },
  });

  const submitMutation = useMutation({
    mutationFn: async () => {
      if (!openChangeset) {
        throw new Error('No open changeset');
      }
      if (isDraftState(openChangeset.state)) {
        return submitChangeset(repoId!, openChangeset.id);
      }
      return resubmitChangeset(repoId!, openChangeset.id);
    },
    onSuccess: async () => {
      toast({ type: 'success', title: 'Changeset submitted' });
      await openChangesetQuery.refetch();
    },
    onError: (error) => {
      toast({
        type: 'error',
        title: 'Submit failed',
        subtitle: error instanceof Error ? error.message : 'Unknown error',
      });
    },
  });

  async function refreshAll() {
    await Promise.all([workspacesQuery.refetch(), changesQuery.refetch(), openChangesetQuery.refetch()]);
    if (selectedPath) {
      await patchQuery.refetch();
    }
  }

  if (repoError) {
    return (
      <div className="flex flex-1 items-center justify-center p-6">
        <Card className="max-w-md">
          <CardHeader>
            <CardTitle className="flex items-center gap-2 text-destructive">
              <AlertCircle className="size-5" />
              Cannot open My Changes
            </CardTitle>
          </CardHeader>
          <CardContent>
            <p className="text-sm text-muted-foreground">{repoError}</p>
          </CardContent>
        </Card>
      </div>
    );
  }

  if (workspacesQuery.isLoading) {
    return (
      <div className="flex flex-1 items-center justify-center p-6">
        <div className="space-y-3">
          <Skeleton className="h-6 w-56" />
          <Skeleton className="h-4 w-80" />
        </div>
      </div>
    );
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
                : 'Unknown error'}
            </p>
          </CardContent>
        </Card>
      </div>
    );
  }

  if (!workspacesQuery.data || workspacesQuery.data.length === 0 || !selectedWorkspace) {
    return (
      <div className="flex flex-1 items-center justify-center p-6">
        <Card className="max-w-md">
          <CardHeader>
            <CardTitle className="flex items-center gap-2 text-destructive">
              <AlertCircle className="size-5" />
              No workspace found
            </CardTitle>
          </CardHeader>
          <CardContent>
            <p className="text-sm text-muted-foreground">
              Create a workspace first to track changes.
            </p>
          </CardContent>
        </Card>
      </div>
    );
  }

  return (
    <div className="flex h-[calc(100vh-3.5rem)] min-w-0 flex-col">
      <div className="border-b px-4 py-3">
        <div className="flex flex-wrap items-center gap-3">
          <div className="min-w-0 flex-1">
            <h1 className="text-sm font-semibold">My Changes</h1>
            <p className="truncate text-xs text-muted-foreground">
              Instance: {repoName} · Workspace: {workspaceLabel(selectedWorkspace)}
            </p>
          </div>

          {workspacesQuery.data.length > 1 ? (
            <select
              aria-label="Select workspace"
              className="h-8 rounded-md border border-input bg-background px-2 text-sm"
              value={selectedWorkspace.id}
              onChange={(event) => setSelectedWorkspaceId(event.target.value)}
            >
              {workspacesQuery.data.map((workspace) => (
                <option key={workspace.id} value={workspace.id}>
                  {workspaceLabel(workspace)}
                </option>
              ))}
            </select>
          ) : null}

          <Button
            variant="outline"
            size="sm"
            className="h-8 gap-1"
            onClick={() => void refreshAll()}
          >
            <RefreshCw className="size-3.5" />
            Refresh
          </Button>
        </div>
      </div>

      <div className="grid min-h-0 flex-1 gap-4 p-4 lg:grid-cols-[360px_minmax(0,1fr)]">
        <div className="flex min-h-0 flex-col gap-4">
          <Card>
            <CardHeader className="pb-3">
              <CardTitle className="text-sm">Draft Changeset</CardTitle>
            </CardHeader>
            <CardContent className="space-y-3">
              {!openChangeset ? (
                <>
                  <Input
                    value={draftTitle}
                    onChange={(event) => setDraftTitle(event.target.value)}
                    placeholder="Title"
                  />
                  <Textarea
                    value={draftDescription}
                    onChange={(event) => setDraftDescription(event.target.value)}
                    placeholder="Description (optional)"
                    className="min-h-24"
                  />
                  <Button
                    size="sm"
                    disabled={!draftTitle.trim() || createDraftMutation.isPending}
                    onClick={() => createDraftMutation.mutate()}
                  >
                    Create Draft
                  </Button>
                </>
              ) : (
                <>
                  <div className="flex items-center gap-2">
                    <Badge variant="outline">{openChangeset.state}</Badge>
                    <span className="text-xs text-muted-foreground">
                      Revision {openChangeset.revision}
                    </span>
                  </div>
                  <Input
                    value={editTitle}
                    onChange={(event) => setEditTitle(event.target.value)}
                    placeholder="Title"
                  />
                  <Textarea
                    value={editDescription}
                    onChange={(event) => setEditDescription(event.target.value)}
                    placeholder="Description"
                    className="min-h-24"
                  />
                  <div className="flex flex-wrap items-center gap-2">
                    <Button
                      size="sm"
                      variant="outline"
                      disabled={!editTitle.trim() || updateMetadataMutation.isPending}
                      onClick={() => updateMetadataMutation.mutate()}
                    >
                      Save metadata
                    </Button>
                    <Button
                      size="sm"
                      disabled={
                        !canSubmitOrResubmit(openChangeset.state) || submitMutation.isPending
                      }
                      onClick={() => submitMutation.mutate()}
                    >
                      {isDraftState(openChangeset.state) ? 'Submit' : 'Resubmit'}
                    </Button>
                  </div>
                  <p className="text-xs text-muted-foreground">
                    Head: {shortSha(openChangeset.head_sha)} · Updated:{' '}
                    {new Date(openChangeset.updated_at).toLocaleString()}
                  </p>
                </>
              )}
            </CardContent>
          </Card>

          <Card className="min-h-0 flex-1">
            <CardHeader className="pb-3">
              <CardTitle className="text-sm">Changed Files</CardTitle>
              <p className="text-xs text-muted-foreground">
                {changesQuery.data
                  ? `${changesQuery.data.files_changed} files · +${changesQuery.data.additions} -${changesQuery.data.deletions}`
                  : 'Loading change summary...'}
              </p>
            </CardHeader>
            <CardContent className="flex min-h-0 flex-col gap-3">
              <Input
                value={filter}
                onChange={(event) => setFilter(event.target.value)}
                placeholder="Filter by path"
              />
              <ScrollArea className="min-h-0 flex-1 rounded-md border">
                <div className="divide-y">
                  {changesQuery.isLoading ? (
                    Array.from({ length: 8 }).map((_, index) => (
                      <div key={index} className="p-2">
                        <Skeleton className="h-4 w-full" />
                      </div>
                    ))
                  ) : filteredEntries.length > 0 ? (
                    filteredEntries.map((entry) => (
                      <button
                        key={entry.path}
                        className={`flex w-full items-center justify-between gap-2 px-3 py-2 text-left text-xs hover:bg-muted/70 ${
                          selectedPath === entry.path ? 'bg-muted' : ''
                        }`}
                        onClick={() => setSelectedPath(entry.path)}
                      >
                        <span className="min-w-0 truncate">{entry.path}</span>
                        <span className="shrink-0 text-muted-foreground">
                          +{entry.additions} -{entry.deletions}
                        </span>
                      </button>
                    ))
                  ) : (
                    <div className="p-3 text-xs text-muted-foreground">No matching files.</div>
                  )}
                </div>
              </ScrollArea>
            </CardContent>
          </Card>
        </div>

        <Card className="min-h-0">
          <CardHeader className="pb-3">
            <CardTitle className="flex items-center gap-2 text-sm">
              <FileDiff className="size-4" />
              Patch
            </CardTitle>
            <p className="truncate text-xs text-muted-foreground">
              {selectedPath ?? 'Select a changed file to inspect its patch'}
            </p>
          </CardHeader>
          <CardContent className="min-h-0">
            <ScrollArea className="h-[calc(100vh-16rem)] rounded-md border bg-muted/10">
              {!selectedPath ? (
                <div className="p-4 text-sm text-muted-foreground">No file selected.</div>
              ) : patchQuery.isLoading ? (
                <div className="space-y-2 p-4">
                  {Array.from({ length: 10 }).map((_, index) => (
                    <Skeleton key={index} className="h-4 w-full" />
                  ))}
                </div>
              ) : patchQuery.error ? (
                <div className="p-4 text-sm text-destructive">
                  {patchQuery.error instanceof ApiError
                    ? patchQuery.error.message
                    : 'Failed to load patch.'}
                </div>
              ) : patchQuery.data?.binary ? (
                <div className="p-4 text-sm text-muted-foreground">
                  Binary file change. No text patch available.
                </div>
              ) : (
                <pre className="overflow-auto p-4 font-mono text-xs leading-relaxed text-foreground">
                  {patchQuery.data?.patch || 'No textual diff output for this file.'}
                </pre>
              )}
            </ScrollArea>
          </CardContent>
        </Card>
      </div>
    </div>
  );
}

import { useCallback, useEffect, useMemo, useState } from 'react';
import { useQuery } from '@tanstack/react-query';
import { FilePlus, FolderPlus } from 'lucide-react';
import { Skeleton } from '@/components/ui/skeleton';
import { Button } from '@/components/ui/button';
import {
  Empty,
  EmptyContent,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from '@/components/ui/empty';
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from '@/components/ui/tooltip';
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuTrigger,
} from '@/components/ui/context-menu';
import { ApiError } from '~/api/client';
import { getFileTree } from '../workspace-api';
import type { FileEntry, PendingCreation, TreeNode } from '../workspace-types';
import FileTreeNode from './file-tree-node';
import FileTreeInput from './file-tree-input';

interface FileTreeProps {
  repoId: string;
  workspaceId: string;
  selectedFile: string | null;
  onFileSelect: (path: string) => void;
  pendingCreation: PendingCreation | null;
  onRequestCreate: (type: 'file' | 'folder', parentPath: string) => void;
  onCreateConfirm: (fullPath: string, type: 'file' | 'folder') => void;
  onCreateCancel: () => void;
  onDelete: (path: string) => void;
}

function storageKey(repoId: string, workspaceId: string): string {
  return `conman.file-tree.expanded.${repoId}.${workspaceId}`;
}

function isMissingRevisionError(error: unknown): boolean {
  return (
    error instanceof ApiError &&
    error.code === 'git_error' &&
    (error.message.includes('Needed a single revision') ||
      error.message.includes('bad revision') ||
      error.message.includes('Not a valid object name'))
  );
}

function isTreeUnsupportedError(error: unknown): boolean {
  return (
    error instanceof ApiError &&
    error.code === 'git_error' &&
    error.message.includes('get_tree_entries') &&
    error.message.includes('not implemented')
  );
}

// Build a nested tree from the flat list of file entries
function buildTree(entries: FileEntry[]): TreeNode[] {
  const root: TreeNode[] = [];
  const seen = new Set<string>();

  // Recursive listings from git can omit explicit directory entries.
  // Build missing folder nodes from path prefixes and de-duplicate nodes.
  for (const entry of entries) {
    const normalizedPath = entry.path.replace(/^\/+|\/+$/g, '');
    if (!normalizedPath) continue;

    const parts = normalizedPath.split('/').filter(Boolean);
    let children = root;
    let currentPath = '';

    for (let i = 0; i < parts.length; i += 1) {
      const part = parts[i];
      const isLeaf = i === parts.length - 1;
      const desiredType: 'file' | 'dir' = isLeaf ? entry.entry_type : 'dir';

      currentPath = currentPath ? `${currentPath}/${part}` : part;
      const dedupeKey = `${desiredType}:${currentPath}`;
      let node = children.find((candidate) => candidate.path === currentPath);

      if (!node) {
        node = {
          name: part,
          path: currentPath,
          type: desiredType,
          ...(desiredType === 'dir' ? { children: [] } : {}),
        };
        if (!seen.has(dedupeKey)) {
          children.push(node);
          seen.add(dedupeKey);
        }
      } else if (desiredType === 'dir' && node.type !== 'dir') {
        node.type = 'dir';
        node.children = node.children ?? [];
      }

      if (node.type === 'dir') {
        node.children = node.children ?? [];
        children = node.children;
      }
    }
  }

  // Sort children within each directory: dirs first, then alpha
  function sortChildren(nodes: TreeNode[]) {
    nodes.sort((a, b) => {
      if (a.type !== b.type) return a.type === 'dir' ? -1 : 1;
      return a.name.localeCompare(b.name);
    });
    for (const node of nodes) {
      if (node.children) sortChildren(node.children);
    }
  }
  sortChildren(root);

  return root;
}

export default function FileTree({
  repoId,
  workspaceId,
  selectedFile,
  onFileSelect,
  pendingCreation,
  onRequestCreate,
  onCreateConfirm,
  onCreateCancel,
  onDelete,
}: FileTreeProps) {
  const [expandedPaths, setExpandedPaths] = useState<Set<string>>(new Set());

  const expansionStorageKey = useMemo(
    () => storageKey(repoId, workspaceId),
    [repoId, workspaceId],
  );

  useEffect(() => {
    if (typeof window === 'undefined') {
      return;
    }
    const raw = window.localStorage.getItem(expansionStorageKey);
    if (!raw) {
      setExpandedPaths(new Set());
      return;
    }
    try {
      const parsed = JSON.parse(raw) as string[];
      setExpandedPaths(new Set(parsed));
    } catch {
      setExpandedPaths(new Set());
    }
  }, [expansionStorageKey]);

  useEffect(() => {
    if (typeof window === 'undefined') {
      return;
    }
    window.localStorage.setItem(
      expansionStorageKey,
      JSON.stringify(Array.from(expandedPaths)),
    );
  }, [expandedPaths, expansionStorageKey]);

  const isPathExpanded = useCallback(
    (path: string) => expandedPaths.has(path),
    [expandedPaths],
  );

  const onToggleDir = useCallback((path: string) => {
    setExpandedPaths((previous) => {
      const next = new Set(previous);
      if (next.has(path)) {
        next.delete(path);
      } else {
        next.add(path);
      }
      return next;
    });
  }, []);

  const treeQuery = useQuery({
    queryKey: ['file-tree', repoId, workspaceId],
    queryFn: () => getFileTree(repoId, workspaceId, '', true),
    retry(failureCount, error) {
      if (isTreeUnsupportedError(error) || isMissingRevisionError(error)) {
        return false;
      }
      return failureCount < 2;
    },
  });

  const tree = useMemo(() => {
    if (isMissingRevisionError(treeQuery.error)) {
      return [];
    }
    if (!treeQuery.data?.entries) return [];
    return buildTree(treeQuery.data.entries);
  }, [treeQuery.data, treeQuery.error]);

  // Sticky header with "Files" label and create buttons
  const header = (
    <div className="sticky top-0 z-10 flex items-center justify-between bg-background/95 px-3 py-2 backdrop-blur-sm">
      <span className="text-xs font-medium uppercase text-muted-foreground">Files</span>
      <div className="flex items-center gap-0.5">
        <Tooltip>
          <TooltipTrigger asChild>
            <Button
              variant="ghost"
              size="icon"
              className="size-6"
              onClick={() => onRequestCreate('file', '')}
            >
              <FilePlus className="size-3.5" />
            </Button>
          </TooltipTrigger>
          <TooltipContent side="bottom">New File</TooltipContent>
        </Tooltip>
        <Tooltip>
          <TooltipTrigger asChild>
            <Button
              variant="ghost"
              size="icon"
              className="size-6"
              onClick={() => onRequestCreate('folder', '')}
            >
              <FolderPlus className="size-3.5" />
            </Button>
          </TooltipTrigger>
          <TooltipContent side="bottom">New Folder</TooltipContent>
        </Tooltip>
      </div>
    </div>
  );

  if (treeQuery.isLoading) {
    return (
      <div>
        {header}
        <div className="space-y-2 p-3">
          {Array.from({ length: 8 }).map((_, i) => (
            <Skeleton key={i} className="h-5 w-full" />
          ))}
        </div>
      </div>
    );
  }

  if (treeQuery.error) {
    if (isMissingRevisionError(treeQuery.error)) {
      // Empty workspace — show polished empty state
      return (
        <div>
          {header}
          <Empty className="border-none p-6">
            <EmptyHeader>
              <EmptyMedia variant="icon">
                <FolderPlus />
              </EmptyMedia>
              <EmptyTitle>No files yet</EmptyTitle>
              <EmptyDescription>
                Create your first file to get started.
              </EmptyDescription>
            </EmptyHeader>
            <EmptyContent>
              <Button
                variant="outline"
                size="sm"
                onClick={() => onRequestCreate('file', '')}
              >
                <FilePlus className="mr-1.5 size-4" />
                New File
              </Button>
            </EmptyContent>
          </Empty>
          {/* Render inline input for root-level creation even in empty state */}
          {pendingCreation?.parentPath === '' && (
            <FileTreeInput
              type={pendingCreation.type}
              depth={0}
              onConfirm={(name) => onCreateConfirm(name, pendingCreation.type)}
              onCancel={onCreateCancel}
            />
          )}
        </div>
      );
    }

    // Unsupported environment — preserve existing neutral text
    if (isTreeUnsupportedError(treeQuery.error)) {
      return (
        <div>
          {header}
          <div className="p-3 text-xs leading-relaxed text-muted-foreground">
            File browser is unavailable in this environment.
          </div>
        </div>
      );
    }

    // Generic error fallback
    const message =
      treeQuery.error instanceof Error
        ? treeQuery.error.message
        : 'Failed to load file tree';

    return (
      <div>
        {header}
        <div className="break-words p-3 text-sm text-destructive line-clamp-3">{message}</div>
      </div>
    );
  }

  // Tree loaded but empty (no entries)
  if (tree.length === 0) {
    return (
      <div>
        {header}
        <Empty className="border-none p-6">
          <EmptyHeader>
            <EmptyMedia variant="icon">
              <FolderPlus />
            </EmptyMedia>
            <EmptyTitle>No files yet</EmptyTitle>
            <EmptyDescription>
              Create your first file to get started.
            </EmptyDescription>
          </EmptyHeader>
          <EmptyContent>
            <Button
              variant="outline"
              size="sm"
              onClick={() => onRequestCreate('file', '')}
            >
              <FilePlus className="mr-1.5 size-4" />
              New File
            </Button>
          </EmptyContent>
        </Empty>
        {pendingCreation?.parentPath === '' && (
          <FileTreeInput
            type={pendingCreation.type}
            depth={0}
            onConfirm={(name) => onCreateConfirm(name, pendingCreation.type)}
            onCancel={onCreateCancel}
          />
        )}
      </div>
    );
  }

  return (
    <div>
      {header}
      {/* Root-level context menu wraps the tree content */}
      <ContextMenu>
        <ContextMenuTrigger asChild>
          <div className="py-1">
            {/* Inline input at root level */}
            {pendingCreation?.parentPath === '' && (
              <FileTreeInput
                type={pendingCreation.type}
                depth={0}
                onConfirm={(name) => onCreateConfirm(name, pendingCreation.type)}
                onCancel={onCreateCancel}
              />
            )}
            {tree.map((node) => (
              <FileTreeNode
                key={node.path}
                node={node}
                depth={0}
                isPathExpanded={isPathExpanded}
                onToggleDir={onToggleDir}
                selectedFile={selectedFile}
                onFileSelect={onFileSelect}
                pendingCreation={pendingCreation}
                onRequestCreate={onRequestCreate}
                onCreateConfirm={onCreateConfirm}
                onCreateCancel={onCreateCancel}
                onDelete={onDelete}
              />
            ))}
          </div>
        </ContextMenuTrigger>
        <ContextMenuContent>
          <ContextMenuItem onSelect={() => onRequestCreate('file', '')}>
            <FilePlus className="size-4" />
            New File
          </ContextMenuItem>
          <ContextMenuItem onSelect={() => onRequestCreate('folder', '')}>
            <FolderPlus className="size-4" />
            New Folder
          </ContextMenuItem>
        </ContextMenuContent>
      </ContextMenu>
    </div>
  );
}

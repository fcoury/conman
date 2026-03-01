import { useMemo } from 'react';
import { useQuery } from '@tanstack/react-query';
import { Skeleton } from '@/components/ui/skeleton';
import { ApiError } from '~/api/client';
import { getFileTree } from '../workspace-api';
import type { FileEntry, TreeNode } from '../workspace-types';
import FileTreeNode from './file-tree-node';

interface FileTreeProps {
  repoId: string;
  workspaceId: string;
  selectedFile: string | null;
  onFileSelect: (path: string) => void;
}

// Build a nested tree from the flat list of file entries
function buildTree(entries: FileEntry[]): TreeNode[] {
  const root: TreeNode[] = [];

  // Sort: directories first, then alphabetically
  const sorted = [...entries].sort((a, b) => {
    if (a.entry_type !== b.entry_type) {
      return a.entry_type === 'dir' ? -1 : 1;
    }
    return a.path.localeCompare(b.path);
  });

  const dirMap = new Map<string, TreeNode>();

  for (const entry of sorted) {
    const parts = entry.path.split('/');
    const name = parts[parts.length - 1];
    const node: TreeNode = {
      name,
      path: entry.path,
      type: entry.entry_type,
      ...(entry.entry_type === 'dir' ? { children: [] } : {}),
    };

    if (entry.entry_type === 'dir') {
      dirMap.set(entry.path, node);
    }

    // Find parent directory
    const parentPath = parts.slice(0, -1).join('/');
    const parent = parentPath ? dirMap.get(parentPath) : null;

    if (parent?.children) {
      parent.children.push(node);
    } else if (!parentPath) {
      root.push(node);
    } else {
      // Parent directory wasn't in the list — place at root
      root.push(node);
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
}: FileTreeProps) {
  const treeQuery = useQuery({
    queryKey: ['file-tree', repoId, workspaceId],
    queryFn: () => getFileTree(repoId, workspaceId, '', true),
    retry(failureCount, error) {
      if (
        error instanceof ApiError &&
        error.code === 'git_error' &&
        error.message.includes('not implemented')
      ) {
        return false;
      }
      return failureCount < 2;
    },
  });

  const tree = useMemo(() => {
    if (!treeQuery.data?.entries) return [];
    return buildTree(treeQuery.data.entries);
  }, [treeQuery.data]);

  if (treeQuery.isLoading) {
    return (
      <div className="space-y-2 p-3">
        {Array.from({ length: 8 }).map((_, i) => (
          <Skeleton key={i} className="h-5 w-full" />
        ))}
      </div>
    );
  }

  if (treeQuery.error) {
    let message = 'Failed to load file tree';
    if (
      treeQuery.error instanceof ApiError &&
      treeQuery.error.code === 'git_error' &&
      treeQuery.error.message.includes('not implemented')
    ) {
      message = 'File tree unavailable: backend get_tree_entries is not implemented yet';
    } else if (treeQuery.error instanceof Error) {
      message = treeQuery.error.message;
    }

    return (
      <div className="p-3 text-sm text-destructive">{message}</div>
    );
  }

  if (tree.length === 0) {
    return (
      <div className="p-3 text-sm text-muted-foreground">
        No files in this workspace
      </div>
    );
  }

  return (
    <div className="py-1">
      {tree.map((node) => (
        <FileTreeNode
          key={node.path}
          node={node}
          depth={0}
          selectedFile={selectedFile}
          onFileSelect={onFileSelect}
        />
      ))}
    </div>
  );
}

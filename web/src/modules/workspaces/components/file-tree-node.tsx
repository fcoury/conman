import { useState } from 'react';
import {
  ChevronRight,
  File,
  FileCode,
  FileJson,
  FilePlus,
  FileText,
  FileType,
  Folder,
  FolderOpen,
  FolderPlus,
  Image,
  Trash2,
} from 'lucide-react';
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuTrigger,
} from '@/components/ui/context-menu';
import type { PendingCreation, TreeNode } from '../workspace-types';
import FileTreeInput from './file-tree-input';

interface FileTreeNodeProps {
  node: TreeNode;
  depth: number;
  selectedFile: string | null;
  onFileSelect: (path: string) => void;
  pendingCreation: PendingCreation | null;
  onRequestCreate: (type: 'file' | 'folder', parentPath: string) => void;
  onCreateConfirm: (fullPath: string, type: 'file' | 'folder') => void;
  onCreateCancel: () => void;
  onDelete: (path: string) => void;
}

// Pick an icon based on file extension
function fileIcon(name: string) {
  const ext = name.split('.').pop()?.toLowerCase() ?? '';
  switch (ext) {
    case 'json':
      return FileJson;
    case 'js':
    case 'jsx':
    case 'ts':
    case 'tsx':
    case 'rs':
    case 'go':
    case 'py':
    case 'rb':
      return FileCode;
    case 'md':
    case 'txt':
    case 'toml':
    case 'yaml':
    case 'yml':
      return FileText;
    case 'png':
    case 'jpg':
    case 'jpeg':
    case 'gif':
    case 'svg':
    case 'webp':
    case 'ico':
      return Image;
    case 'ttf':
    case 'woff':
    case 'woff2':
    case 'otf':
      return FileType;
    default:
      return File;
  }
}

export default function FileTreeNode({
  node,
  depth,
  selectedFile,
  onFileSelect,
  pendingCreation,
  onRequestCreate,
  onCreateConfirm,
  onCreateCancel,
  onDelete,
}: FileTreeNodeProps) {
  const [expanded, setExpanded] = useState(depth < 1);
  const isDir = node.type === 'dir';
  const isSelected = !isDir && node.path === selectedFile;

  // Auto-expand folder when it's the target of a pending creation
  const isCreationTarget = pendingCreation?.parentPath === node.path && isDir;
  const isExpanded = expanded || isCreationTarget;

  function handleClick() {
    if (isDir) {
      setExpanded((prev) => !prev);
    } else {
      onFileSelect(node.path);
    }
  }

  // Build the full path for a new child item
  function handleChildConfirm(name: string) {
    if (!pendingCreation) return;
    const fullPath = pendingCreation.parentPath
      ? pendingCreation.parentPath + '/' + name
      : name;
    onCreateConfirm(fullPath, pendingCreation.type);
  }

  const Icon = isDir
    ? isExpanded
      ? FolderOpen
      : Folder
    : fileIcon(node.name);

  const button = (
    <button
      onClick={handleClick}
      className={`flex w-full items-center gap-1.5 px-2 py-1 text-left text-sm hover:bg-accent/50 ${
        isSelected ? 'bg-accent text-accent-foreground' : 'text-foreground/80'
      }`}
      style={{ paddingLeft: `${depth * 16 + 8}px` }}
    >
      {isDir ? (
        <ChevronRight
          className={`size-3.5 shrink-0 transition-transform ${isExpanded ? 'rotate-90' : ''}`}
        />
      ) : (
        <span className="w-3.5 shrink-0" />
      )}
      <Icon className="size-4 shrink-0 text-muted-foreground" />
      <span className="truncate">{node.name}</span>
    </button>
  );

  // Context menu items differ by node type
  const contextMenuContent = isDir ? (
    <ContextMenuContent>
      <ContextMenuItem onSelect={() => onRequestCreate('file', node.path)}>
        <FilePlus className="size-4" />
        New File
      </ContextMenuItem>
      <ContextMenuItem onSelect={() => onRequestCreate('folder', node.path)}>
        <FolderPlus className="size-4" />
        New Folder
      </ContextMenuItem>
    </ContextMenuContent>
  ) : (
    <ContextMenuContent>
      <ContextMenuItem variant="destructive" onSelect={() => onDelete(node.path)}>
        <Trash2 className="size-4" />
        Delete
      </ContextMenuItem>
    </ContextMenuContent>
  );

  return (
    <div>
      <ContextMenu>
        <ContextMenuTrigger asChild>
          {button}
        </ContextMenuTrigger>
        {contextMenuContent}
      </ContextMenu>

      {isDir && isExpanded && (
        <div>
          {/* Inline input for creating inside this folder */}
          {isCreationTarget && pendingCreation && (
            <FileTreeInput
              type={pendingCreation.type}
              depth={depth + 1}
              onConfirm={handleChildConfirm}
              onCancel={onCreateCancel}
            />
          )}
          {node.children?.map((child) => (
            <FileTreeNode
              key={child.path}
              node={child}
              depth={depth + 1}
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
      )}
    </div>
  );
}

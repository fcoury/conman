import { useState } from 'react';
import {
  ChevronRight,
  File,
  FileCode,
  FileJson,
  FileText,
  FileType,
  Folder,
  FolderOpen,
  Image,
} from 'lucide-react';
import type { TreeNode } from '../workspace-types';

interface FileTreeNodeProps {
  node: TreeNode;
  depth: number;
  selectedFile: string | null;
  onFileSelect: (path: string) => void;
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
}: FileTreeNodeProps) {
  const [expanded, setExpanded] = useState(depth < 1);
  const isDir = node.type === 'dir';
  const isSelected = !isDir && node.path === selectedFile;

  function handleClick() {
    if (isDir) {
      setExpanded((prev) => !prev);
    } else {
      onFileSelect(node.path);
    }
  }

  const Icon = isDir
    ? expanded
      ? FolderOpen
      : Folder
    : fileIcon(node.name);

  return (
    <div>
      <button
        onClick={handleClick}
        className={`flex w-full items-center gap-1.5 px-2 py-1 text-left text-sm hover:bg-accent/50 ${
          isSelected ? 'bg-accent text-accent-foreground' : 'text-foreground/80'
        }`}
        style={{ paddingLeft: `${depth * 16 + 8}px` }}
      >
        {isDir ? (
          <ChevronRight
            className={`size-3.5 shrink-0 transition-transform ${expanded ? 'rotate-90' : ''}`}
          />
        ) : (
          <span className="w-3.5 shrink-0" />
        )}
        <Icon className="size-4 shrink-0 text-muted-foreground" />
        <span className="truncate">{node.name}</span>
      </button>

      {isDir && expanded && node.children && (
        <div>
          {node.children.map((child) => (
            <FileTreeNode
              key={child.path}
              node={child}
              depth={depth + 1}
              selectedFile={selectedFile}
              onFileSelect={onFileSelect}
            />
          ))}
        </div>
      )}
    </div>
  );
}

import { useCallback, useState } from 'react';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { FileText } from 'lucide-react';
import {
  ResizableHandle,
  ResizablePanel,
  ResizablePanelGroup,
} from '@/components/ui/resizable';
import { ScrollArea } from '@/components/ui/scroll-area';
import {
  Empty,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from '@/components/ui/empty';
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/lib/dialog';
import { Button } from '@/components/ui/button';
import { toast } from '@/lib/toast/toast';

import { useWorkspaceContext, WorkspaceContextProvider } from './workspace-context';
import { deleteFile, getFileContent, writeFile } from './workspace-api';
import type { PendingCreation } from './workspace-types';
import FileTree from './components/file-tree';
import FileEditor from './components/file-editor';
import FilePreview from './components/file-preview';
import WorkspaceToolbar from './components/workspace-toolbar';

// Determine if a file extension is editable text
function isTextFile(path: string): boolean {
  const ext = path.split('.').pop()?.toLowerCase() ?? '';
  const textExts = new Set([
    'yaml', 'yml', 'json', 'js', 'jsx', 'ts', 'tsx', 'css', 'scss',
    'html', 'htm', 'xml', 'svg', 'md', 'txt', 'toml', 'ini', 'cfg',
    'sh', 'bash', 'zsh', 'fish', 'env', 'tf', 'gitignore', 'dockerignore',
    'dockerfile', 'makefile', 'rs', 'go', 'py', 'rb', 'lua',
  ]);
  // Also consider files with no extension as text
  if (!ext || ext === path) return true;
  return textExts.has(ext);
}

// Determine if a file is a previewable image
function isImageFile(path: string): boolean {
  const ext = path.split('.').pop()?.toLowerCase() ?? '';
  return ['png', 'jpg', 'jpeg', 'gif', 'svg', 'webp', 'ico'].includes(ext);
}

// Determine if a file is a font
function isFontFile(path: string): boolean {
  const ext = path.split('.').pop()?.toLowerCase() ?? '';
  return ['ttf', 'woff', 'woff2', 'otf'].includes(ext);
}

type SaveStatus = 'idle' | 'saving' | 'saved' | 'error';

function WorkspaceViewInner() {
  const { repoId, workspace } = useWorkspaceContext();
  const queryClient = useQueryClient();
  const [selectedFile, setSelectedFile] = useState<string | null>(null);
  const [isDirty, setIsDirty] = useState(false);
  const [saveStatus, setSaveStatus] = useState<SaveStatus>('idle');
  const [editorContent, setEditorContent] = useState<string>('');
  const [pendingCreation, setPendingCreation] = useState<PendingCreation | null>(null);
  const [deleteTarget, setDeleteTarget] = useState<{ path: string; type: 'file' } | null>(null);

  const treeQueryKey = ['file-tree', repoId, workspace.id];

  // Fetch file content when a file is selected
  const fileQuery = useQuery({
    queryKey: ['file-content', repoId, workspace.id, selectedFile],
    queryFn: () => getFileContent(repoId, workspace.id, selectedFile!),
    enabled: !!selectedFile,
  });

  // Decode base64 content to text
  const decodedContent = fileQuery.data
    ? atob(fileQuery.data.content)
    : null;

  // Handle file selection from tree
  const handleFileSelect = useCallback((path: string) => {
    setSelectedFile(path);
    setIsDirty(false);
    setSaveStatus('idle');
  }, []);

  // Handle content changes in editor
  const handleContentChange = useCallback(
    (content: string) => {
      setEditorContent(content);
      setIsDirty(true);
      setSaveStatus('idle');
    },
    [],
  );

  // Handle save (Ctrl+S)
  const handleSave = useCallback(async () => {
    if (!selectedFile) return;
    setSaveStatus('saving');
    try {
      const encoded = btoa(editorContent);
      await writeFile(repoId, workspace.id, selectedFile, encoded);
      setSaveStatus('saved');
      setIsDirty(false);
      // Auto-clear saved status after 2s
      setTimeout(() => setSaveStatus((s) => (s === 'saved' ? 'idle' : s)), 2000);
    } catch {
      setSaveStatus('error');
    }
  }, [editorContent, repoId, selectedFile, workspace.id]);

  // Request inline creation input
  const handleRequestCreate = useCallback((type: 'file' | 'folder', parentPath: string) => {
    setPendingCreation({ type, parentPath });
  }, []);

  // Confirm file/folder creation
  const handleCreateConfirm = useCallback(async (fullPath: string, type: 'file' | 'folder') => {
    setPendingCreation(null);
    try {
      if (type === 'folder') {
        // Git requires a file inside the folder
        await writeFile(repoId, workspace.id, fullPath + '/.gitkeep', btoa(''));
      } else {
        await writeFile(repoId, workspace.id, fullPath, btoa(''));
        // Auto-select newly created file
        setSelectedFile(fullPath);
        setIsDirty(false);
        setSaveStatus('idle');
      }
      await queryClient.invalidateQueries({ queryKey: treeQueryKey });
      toast({ type: 'success', title: `${type === 'folder' ? 'Folder' : 'File'} created` });
    } catch (err) {
      toast({
        type: 'error',
        title: 'Creation failed',
        subtitle: (err as Error).message,
      });
    }
  }, [repoId, workspace.id, queryClient, treeQueryKey]);

  // Cancel inline creation
  const handleCreateCancel = useCallback(() => {
    setPendingCreation(null);
  }, []);

  // Request file deletion (opens confirmation dialog)
  const handleDeleteRequest = useCallback((path: string) => {
    setDeleteTarget({ path, type: 'file' });
  }, []);

  // Confirm file deletion
  const handleDeleteConfirm = useCallback(async () => {
    if (!deleteTarget) return;
    try {
      await deleteFile(repoId, workspace.id, deleteTarget.path);
      await queryClient.invalidateQueries({ queryKey: treeQueryKey });
      // Clear selection if the deleted file was selected
      if (selectedFile === deleteTarget.path) {
        setSelectedFile(null);
        setIsDirty(false);
        setSaveStatus('idle');
      }
      toast({ type: 'success', title: 'File deleted' });
    } catch (err) {
      toast({
        type: 'error',
        title: 'Delete failed',
        subtitle: (err as Error).message,
      });
    } finally {
      setDeleteTarget(null);
    }
  }, [deleteTarget, repoId, workspace.id, queryClient, treeQueryKey, selectedFile]);

  // Determine what to render in the right pane
  function renderContent() {
    // Empty state: no file selected
    if (!selectedFile) {
      return (
        <Empty className="h-full border-none">
          <EmptyHeader>
            <EmptyMedia variant="icon">
              <FileText />
            </EmptyMedia>
            <EmptyTitle>No file open</EmptyTitle>
            <EmptyDescription>
              Select a file from the tree, or create a new one.
            </EmptyDescription>
          </EmptyHeader>
        </Empty>
      );
    }

    if (fileQuery.isLoading) {
      return (
        <div className="flex h-full items-center justify-center text-sm text-muted-foreground">
          Loading...
        </div>
      );
    }

    if (fileQuery.error) {
      return (
        <div className="flex h-full items-center justify-center text-sm text-destructive">
          Failed to load file
        </div>
      );
    }

    if (!fileQuery.data) return null;

    // Image preview
    if (isImageFile(selectedFile)) {
      return (
        <FilePreview
          filePath={selectedFile}
          content={fileQuery.data.content}
          size={fileQuery.data.size}
          type="image"
        />
      );
    }

    // Font preview
    if (isFontFile(selectedFile)) {
      return (
        <FilePreview
          filePath={selectedFile}
          content={fileQuery.data.content}
          size={fileQuery.data.size}
          type="font"
        />
      );
    }

    // Text file editor
    if (isTextFile(selectedFile) && decodedContent !== null) {
      const readOnly = fileQuery.data.size > 500_000;
      return (
        <FileEditor
          content={decodedContent}
          filePath={selectedFile}
          readOnly={readOnly}
          onChange={handleContentChange}
          onSave={handleSave}
        />
      );
    }

    // Fallback: binary download
    return (
      <FilePreview
        filePath={selectedFile}
        content={fileQuery.data.content}
        size={fileQuery.data.size}
        type="binary"
      />
    );
  }

  return (
    <div className="flex min-w-0 h-[calc(100vh-3.5rem)] flex-col">
      <WorkspaceToolbar
        filePath={selectedFile}
        isDirty={isDirty}
        saveStatus={saveStatus}
        onSave={handleSave}
      />
      <ResizablePanelGroup orientation="horizontal" className="flex-1">
        <ResizablePanel defaultSize="25%" minSize="15%" maxSize="40%" className="overflow-hidden">
          <ScrollArea className="h-full">
            <FileTree
              repoId={repoId}
              workspaceId={workspace.id}
              selectedFile={selectedFile}
              onFileSelect={handleFileSelect}
              pendingCreation={pendingCreation}
              onRequestCreate={handleRequestCreate}
              onCreateConfirm={handleCreateConfirm}
              onCreateCancel={handleCreateCancel}
              onDelete={handleDeleteRequest}
            />
          </ScrollArea>
        </ResizablePanel>
        <ResizableHandle withHandle />
        <ResizablePanel defaultSize="75%" className="overflow-hidden">
          {renderContent()}
        </ResizablePanel>
      </ResizablePanelGroup>

      {/* Delete confirmation dialog */}
      <Dialog open={!!deleteTarget} onOpenChange={(open) => !open && setDeleteTarget(null)}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Delete file</DialogTitle>
            <DialogDescription>
              Are you sure you want to delete{' '}
              <span className="font-medium text-foreground">{deleteTarget?.path}</span>?
              This action cannot be undone.
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <DialogClose asChild>
              <Button variant="outline">Cancel</Button>
            </DialogClose>
            <Button variant="destructive" onClick={handleDeleteConfirm}>
              Delete
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}

// Wrapper that provides workspace context
export default function WorkspaceView() {
  return (
    <WorkspaceContextProvider>
      <WorkspaceViewInner />
    </WorkspaceContextProvider>
  );
}

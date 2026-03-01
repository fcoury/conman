import { useCallback, useState } from 'react';
import { useQuery } from '@tanstack/react-query';
import {
  ResizableHandle,
  ResizablePanel,
  ResizablePanelGroup,
} from '@/components/ui/resizable';
import { ScrollArea } from '@/components/ui/scroll-area';

import { useWorkspaceContext, WorkspaceContextProvider } from './workspace-context';
import { getFileContent, writeFile } from './workspace-api';
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
    'sh', 'bash', 'zsh', 'fish', 'env', 'gitignore', 'dockerignore',
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
  const [selectedFile, setSelectedFile] = useState<string | null>(null);
  const [isDirty, setIsDirty] = useState(false);
  const [saveStatus, setSaveStatus] = useState<SaveStatus>('idle');
  const [editorContent, setEditorContent] = useState<string>('');

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

  // Determine what to render in the right pane
  function renderContent() {
    if (!selectedFile) {
      return (
        <div className="flex h-full items-center justify-center text-sm text-muted-foreground">
          Select a file to view
        </div>
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
    <div className="flex h-[calc(100vh-3.5rem)] flex-col">
      <WorkspaceToolbar
        filePath={selectedFile}
        isDirty={isDirty}
        saveStatus={saveStatus}
        onSave={handleSave}
      />
      <ResizablePanelGroup orientation="horizontal" className="flex-1">
        <ResizablePanel defaultSize={25} minSize={15} maxSize={40}>
          <ScrollArea className="h-full">
            <FileTree
              repoId={repoId}
              workspaceId={workspace.id}
              selectedFile={selectedFile}
              onFileSelect={handleFileSelect}
            />
          </ScrollArea>
        </ResizablePanel>
        <ResizableHandle withHandle />
        <ResizablePanel defaultSize={75}>
          {renderContent()}
        </ResizablePanel>
      </ResizablePanelGroup>
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

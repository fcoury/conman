import { FormEvent, useEffect, useMemo, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import Editor from "@monaco-editor/react";
import { load as parseYaml } from "js-yaml";

import { ApiError } from "@/api/client";
import { Button } from "@/components/ui/button";
import { Card, CardDescription, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Select } from "@/components/ui/select";
import { JsonView } from "@/components/ui/json-view";
import { useApi } from "@/hooks/use-api";
import { useRepoContext } from "@/hooks/use-repo-context";
import { fileExtension, formatDate, isProbablyTextFile } from "@/lib/utils";
import { Page } from "@/modules/shared/page";
import type { Workspace } from "@/types/api";

interface FileEntry {
  path: string;
  entry_type: "file" | "dir";
  size: number;
  oid: string;
}

interface FileTreeResponse {
  path: string;
  entries: FileEntry[];
}

interface FileContentResponse {
  path: string;
  content: string;
  size: number;
}

const LARGE_FILE_LIMIT_BYTES = 240_000;

function encodeBase64(content: string): string {
  const bytes = new TextEncoder().encode(content);
  let binary = "";
  for (const byte of bytes) {
    binary += String.fromCharCode(byte);
  }
  return btoa(binary);
}

function decodeBase64(content: string): string {
  const binary = atob(content);
  const bytes = Uint8Array.from(binary, (char) => char.charCodeAt(0));
  return new TextDecoder().decode(bytes);
}

function editorLanguage(path: string): string {
  const extension = fileExtension(path);
  if (extension === "yml" || extension === "yaml") return "yaml";
  if (extension === "json") return "json";
  if (extension === "js" || extension === "mjs" || extension === "cjs") return "javascript";
  if (extension === "ts") return "typescript";
  if (extension === "tsx") return "typescript";
  if (extension === "md") return "markdown";
  if (extension === "css") return "css";
  if (extension === "html") return "html";
  return "plaintext";
}

function validateContent(path: string, content: string): string | null {
  const extension = fileExtension(path);
  try {
    if (extension === "json") {
      JSON.parse(content);
    }
    if (extension === "yml" || extension === "yaml") {
      parseYaml(content);
    }
    return null;
  } catch (error) {
    return error instanceof Error ? error.message : "invalid content";
  }
}

export function WorkspacesPage(): React.ReactElement {
  const api = useApi();
  const queryClient = useQueryClient();
  const context = useRepoContext();
  const repoId = context?.repo?.id;

  const [workspaceTitle, setWorkspaceTitle] = useState("Main Workspace");
  const [workspaceBranch, setWorkspaceBranch] = useState("");
  const [selectedWorkspaceId, setSelectedWorkspaceId] = useState("");
  const [treePath, setTreePath] = useState("");
  const [treeFilter, setTreeFilter] = useState("");
  const [selectedFilePath, setSelectedFilePath] = useState("");
  const [commitMessage, setCommitMessage] = useState("");
  const [editorContent, setEditorContent] = useState("");
  const [localValidationError, setLocalValidationError] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const workspacesQuery = useQuery({
    queryKey: ["workspaces", repoId],
    queryFn: () => api.data<Workspace[]>(`/api/repos/${repoId}/workspaces`),
    enabled: Boolean(repoId),
  });

  useEffect(() => {
    if (!selectedWorkspaceId && workspacesQuery.data?.[0]?.id) {
      setSelectedWorkspaceId(workspacesQuery.data[0].id);
    }
  }, [selectedWorkspaceId, workspacesQuery.data]);

  const selectedWorkspace = useMemo(
    () => workspacesQuery.data?.find((workspace) => workspace.id === selectedWorkspaceId) ?? null,
    [workspacesQuery.data, selectedWorkspaceId],
  );

  const treeQuery = useQuery({
    queryKey: ["workspace-tree", repoId, selectedWorkspaceId, treePath],
    queryFn: () =>
      api.data<FileTreeResponse | FileContentResponse>(
        `/api/repos/${repoId}/workspaces/${selectedWorkspaceId}/files?path=${encodeURIComponent(treePath)}`,
      ),
    enabled: Boolean(repoId && selectedWorkspaceId),
  });

  const fileQuery = useQuery({
    queryKey: ["workspace-file", repoId, selectedWorkspaceId, selectedFilePath],
    queryFn: () =>
      api.data<FileContentResponse>(
        `/api/repos/${repoId}/workspaces/${selectedWorkspaceId}/files?path=${encodeURIComponent(selectedFilePath)}`,
      ),
    enabled: Boolean(repoId && selectedWorkspaceId && selectedFilePath),
  });

  useEffect(() => {
    if (fileQuery.data?.content) {
      setEditorContent(decodeBase64(fileQuery.data.content));
      setLocalValidationError(null);
      setCommitMessage(`update ${selectedFilePath}`);
    }
  }, [fileQuery.data?.content, selectedFilePath]);

  const refresh = async (): Promise<void> => {
    await queryClient.invalidateQueries({ queryKey: ["workspaces", repoId] });
    await queryClient.invalidateQueries({ queryKey: ["workspace-tree", repoId, selectedWorkspaceId, treePath] });
    await queryClient.invalidateQueries({ queryKey: ["workspace-file", repoId, selectedWorkspaceId, selectedFilePath] });
  };

  const createWorkspace = async (event: FormEvent<HTMLFormElement>): Promise<void> => {
    event.preventDefault();
    if (!repoId) return;
    setError(null);
    try {
      const created = await api.data<Workspace>(`/api/repos/${repoId}/workspaces`, {
        method: "POST",
        body: JSON.stringify({ title: workspaceTitle, branch_name: workspaceBranch || null }),
      });
      setSelectedWorkspaceId(created.id);
      await refresh();
    } catch (cause) {
      setError(cause instanceof ApiError ? cause.message : "failed to create workspace");
    }
  };

  const runWorkspaceAction = async (endpoint: string): Promise<void> => {
    if (!repoId || !selectedWorkspaceId) return;
    setError(null);
    try {
      await api.data(`/api/repos/${repoId}/workspaces/${selectedWorkspaceId}/${endpoint}`, {
        method: "POST",
        body: JSON.stringify({}),
      });
      await refresh();
    } catch (cause) {
      setError(cause instanceof ApiError ? cause.message : `failed to ${endpoint}`);
    }
  };

  const saveFile = async (): Promise<void> => {
    if (!repoId || !selectedWorkspaceId || !selectedFilePath) return;
    const validationError = validateContent(selectedFilePath, editorContent);
    if (validationError) {
      setLocalValidationError(validationError);
      return;
    }
    setLocalValidationError(null);
    setError(null);
    try {
      await api.data(`/api/repos/${repoId}/workspaces/${selectedWorkspaceId}/files`, {
        method: "PUT",
        body: JSON.stringify({
          path: selectedFilePath,
          content: encodeBase64(editorContent),
          message: commitMessage || `update ${selectedFilePath}`,
        }),
      });
      await refresh();
    } catch (cause) {
      setError(cause instanceof ApiError ? cause.message : "failed to save file");
    }
  };

  const deleteFile = async (): Promise<void> => {
    if (!repoId || !selectedWorkspaceId || !selectedFilePath) return;
    setError(null);
    try {
      await api.data(`/api/repos/${repoId}/workspaces/${selectedWorkspaceId}/files`, {
        method: "DELETE",
        body: JSON.stringify({ path: selectedFilePath, message: `delete ${selectedFilePath}` }),
      });
      setSelectedFilePath("");
      setEditorContent("");
      await refresh();
    } catch (cause) {
      setError(cause instanceof ApiError ? cause.message : "failed to delete file");
    }
  };

  const rawEntries = useMemo(() => {
    const data = treeQuery.data;
    if (!data || !("entries" in data)) return [];
    return data.entries;
  }, [treeQuery.data]);

  const entries = useMemo(() => {
    const filter = treeFilter.trim().toLowerCase();
    if (!filter) return rawEntries;
    return rawEntries.filter((entry) => entry.path.toLowerCase().includes(filter));
  }, [rawEntries, treeFilter]);

  const fileSize = fileQuery.data?.size ?? 0;
  const editable = selectedFilePath
    ? isProbablyTextFile(selectedFilePath) && fileSize <= LARGE_FILE_LIMIT_BYTES
    : false;

  if (!repoId) {
    return <Page title="Workspaces">Bind a repo first in Setup.</Page>;
  }

  return (
    <Page title="Workspaces" description="Edit repository files with Monaco, guardrails, and workspace actions.">
      {error ? <Card className="border-destructive/40 bg-destructive/10 p-3 text-sm">{error}</Card> : null}

      <div className="grid gap-4 lg:grid-cols-[320px_1fr]">
        <Card className="space-y-3">
          <CardTitle>Workspace Control</CardTitle>
          <form className="space-y-2" onSubmit={(event) => void createWorkspace(event)}>
            <Input value={workspaceTitle} onChange={(event) => setWorkspaceTitle(event.target.value)} placeholder="title" />
            <Input value={workspaceBranch} onChange={(event) => setWorkspaceBranch(event.target.value)} placeholder="branch (optional)" />
            <Button type="submit">Create workspace</Button>
          </form>

          <Select value={selectedWorkspaceId} onChange={(event) => setSelectedWorkspaceId(event.target.value)}>
            <option value="">Select workspace...</option>
            {(workspacesQuery.data ?? []).map((workspace) => (
              <option key={workspace.id} value={workspace.id}>
                {workspace.title || workspace.branch_name}
              </option>
            ))}
          </Select>

          {selectedWorkspace ? (
            <Card className="bg-muted p-2 text-xs">
              <p>Branch: {selectedWorkspace.branch_name}</p>
              <p>Head: {selectedWorkspace.head_sha}</p>
              <p>Updated: {formatDate(selectedWorkspace.updated_at)}</p>
            </Card>
          ) : null}

          <div className="flex flex-wrap gap-2">
            <Button type="button" variant="secondary" onClick={() => void runWorkspaceAction("sync-integration")}>Sync</Button>
            <Button type="button" variant="secondary" onClick={() => void runWorkspaceAction("reset")}>Reset</Button>
            <Button type="button" variant="secondary" onClick={() => void runWorkspaceAction("checkpoints")}>Checkpoint</Button>
          </div>

          <CardTitle className="mt-2">File Tree</CardTitle>
          <Input value={treePath} onChange={(event) => setTreePath(event.target.value)} placeholder="path" />
          <Input value={treeFilter} onChange={(event) => setTreeFilter(event.target.value)} placeholder="filter" />
          <div className="max-h-[440px] space-y-1 overflow-auto">
            {entries.map((entry) => (
              <button
                key={entry.path}
                type="button"
                className="bg-muted hover:bg-accent flex w-full items-center justify-between rounded px-2 py-1 text-left text-xs"
                onClick={() => {
                  if (entry.entry_type === "dir") {
                    setTreePath(entry.path);
                    setSelectedFilePath("");
                    setEditorContent("");
                  } else {
                    setSelectedFilePath(entry.path);
                  }
                }}
              >
                <span className="truncate">{entry.path}</span>
                <span className="text-muted-foreground">{entry.entry_type}</span>
              </button>
            ))}
            {!entries.length ? <p className="text-muted-foreground text-xs">No entries for this path.</p> : null}
          </div>
        </Card>

        <Card className="space-y-3">
          <CardTitle>Editor</CardTitle>
          <CardDescription>
            {selectedFilePath
              ? `Editing ${selectedFilePath}`
              : "Select a file from the tree to open it in the editor."}
          </CardDescription>

          {selectedFilePath ? (
            <>
              {!isProbablyTextFile(selectedFilePath) ? (
                <Card className="border-warning-foreground/30 bg-warning p-3 text-sm">
                  Binary/non-text file detected. Opened in read-only preview mode.
                </Card>
              ) : null}
              {fileSize > LARGE_FILE_LIMIT_BYTES ? (
                <Card className="border-warning-foreground/30 bg-warning p-3 text-sm">
                  File is larger than {LARGE_FILE_LIMIT_BYTES.toLocaleString()} bytes. Editing is disabled for performance.
                </Card>
              ) : null}
              {localValidationError ? (
                <Card className="border-destructive/40 bg-destructive/10 p-3 text-sm">
                  Validation error: {localValidationError}
                </Card>
              ) : null}
              <Input
                value={commitMessage}
                onChange={(event) => setCommitMessage(event.target.value)}
                placeholder="commit message"
              />
              <div className="h-[560px] overflow-hidden rounded-md border">
                <Editor
                  theme="vs-dark"
                  language={editorLanguage(selectedFilePath)}
                  value={editorContent}
                  onChange={(next) => setEditorContent(next ?? "")}
                  options={{
                    readOnly: !editable,
                    minimap: { enabled: false },
                    wordWrap: "on",
                    fontSize: 13,
                    scrollBeyondLastLine: false,
                  }}
                />
              </div>
              <div className="flex flex-wrap gap-2">
                <Button type="button" onClick={() => void saveFile()} disabled={!editable || !selectedFilePath}>
                  Save file
                </Button>
                <Button type="button" variant="danger" onClick={() => void deleteFile()} disabled={!selectedFilePath}>
                  Delete file
                </Button>
              </div>
            </>
          ) : (
            <JsonView value={treeQuery.data ?? { message: "No file selected" }} />
          )}
        </Card>
      </div>
    </Page>
  );
}

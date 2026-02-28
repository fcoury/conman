import { FormEvent, useEffect, useMemo, useState } from "react";
import { useNavigate } from "react-router-dom";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import Editor from "@monaco-editor/react";
import { load as parseYaml } from "js-yaml";

import { ApiError } from "@/api/client";
import { Button } from "@/components/ui/button";
import { Card, CardDescription, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { RawDataPanel } from "@/components/ui/raw-data-panel";
import { Select } from "@/components/ui/select";
import { useApi } from "@/hooks/use-api";
import { useRepoContext } from "@/hooks/use-repo-context";
import { formatRoleLabel } from "@/lib/rbac";
import { fileExtension, formatDate, isProbablyTextFile } from "@/lib/utils";
import { Page } from "@/modules/shared/page";
import { parentPath } from "@/modules/workspaces/workspaces-utils";
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
  if (extension === "ts" || extension === "tsx") return "typescript";
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
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const context = useRepoContext();
  const repoId = context?.repo?.id;
  const role = context?.role;

  const [workspaceTitle, setWorkspaceTitle] = useState("Main Workspace");
  const [workspaceBranch, setWorkspaceBranch] = useState("");
  const [selectedWorkspaceId, setSelectedWorkspaceId] = useState("");
  const [treePath, setTreePath] = useState("");
  const [treeFilter, setTreeFilter] = useState("");
  const [selectedFilePath, setSelectedFilePath] = useState("");
  const [commitMessage, setCommitMessage] = useState("");
  const [editorContent, setEditorContent] = useState("");
  const [loadedContent, setLoadedContent] = useState("");
  const [changesetTitle, setChangesetTitle] = useState("Config update");
  const [changesetDescription, setChangesetDescription] = useState("Created from Draft Changes.");
  const [createdChangesetId, setCreatedChangesetId] = useState<string | null>(null);
  const [status, setStatus] = useState<string | null>(null);
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

  useEffect(() => {
    if (!selectedWorkspace) {
      return;
    }
    const name = selectedWorkspace.title || selectedWorkspace.branch_name;
    setChangesetTitle(`Update ${name}`);
  }, [selectedWorkspace]);

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
      const decoded = decodeBase64(fileQuery.data.content);
      setLoadedContent(decoded);
      setEditorContent(decoded);
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
    setStatus(null);
    try {
      const created = await api.data<Workspace>(`/api/repos/${repoId}/workspaces`, {
        method: "POST",
        body: JSON.stringify({ title: workspaceTitle, branch_name: workspaceBranch || null }),
      });
      setSelectedWorkspaceId(created.id);
      setTreePath("");
      setSelectedFilePath("");
      setEditorContent("");
      setLoadedContent("");
      setStatus("Workspace created.");
      await refresh();
    } catch (cause) {
      setError(cause instanceof ApiError ? cause.message : "failed to create workspace");
    }
  };

  const runWorkspaceAction = async (endpoint: string, successMessage: string): Promise<void> => {
    if (!repoId || !selectedWorkspaceId) return;
    setError(null);
    setStatus(null);
    try {
      await api.data(`/api/repos/${repoId}/workspaces/${selectedWorkspaceId}/${endpoint}`, {
        method: "POST",
        body: JSON.stringify({}),
      });
      setStatus(successMessage);
      await refresh();
    } catch (cause) {
      setError(cause instanceof ApiError ? cause.message : `failed to ${endpoint}`);
    }
  };

  const createChangesetFromWorkspace = async (): Promise<void> => {
    if (!repoId || !selectedWorkspaceId) return;
    setError(null);
    setStatus(null);
    setCreatedChangesetId(null);
    try {
      const created = await api.data<{ id: string }>(`/api/repos/${repoId}/changesets`, {
        method: "POST",
        body: JSON.stringify({
          workspace_id: selectedWorkspaceId,
          title: changesetTitle || "Config update",
          description: changesetDescription || null,
        }),
      });
      setCreatedChangesetId(created.id);
      setStatus("Changeset created from active workspace.");
      await queryClient.invalidateQueries({ queryKey: ["changesets", repoId] });
    } catch (cause) {
      setError(cause instanceof ApiError ? cause.message : "failed to create changeset");
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
    setStatus(null);
    try {
      await api.data(`/api/repos/${repoId}/workspaces/${selectedWorkspaceId}/files`, {
        method: "PUT",
        body: JSON.stringify({
          path: selectedFilePath,
          content: encodeBase64(editorContent),
          message: commitMessage || `update ${selectedFilePath}`,
        }),
      });
      setStatus("File saved to workspace branch.");
      setLoadedContent(editorContent);
      await refresh();
    } catch (cause) {
      setError(cause instanceof ApiError ? cause.message : "failed to save file");
    }
  };

  const deleteFile = async (): Promise<void> => {
    if (!repoId || !selectedWorkspaceId || !selectedFilePath) return;
    setError(null);
    setStatus(null);
    try {
      await api.data(`/api/repos/${repoId}/workspaces/${selectedWorkspaceId}/files`, {
        method: "DELETE",
        body: JSON.stringify({ path: selectedFilePath, message: `delete ${selectedFilePath}` }),
      });
      setStatus("File deleted from workspace branch.");
      setSelectedFilePath("");
      setEditorContent("");
      setLoadedContent("");
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
  const hasUnsavedChanges = selectedFilePath ? editorContent !== loadedContent : false;

  if (!repoId) {
    return <Page title="Workspaces">Bind a repo first in Setup.</Page>;
  }

  return (
    <Page
      title="Draft Changes"
      description="Create a workspace, edit YAML/config files, save your branch commits, then open a changeset for review."
    >
      {error ? <Card className="border-destructive/40 bg-destructive/10 p-3 text-sm">{error}</Card> : null}
      {status ? <Card className="border-success/40 bg-success/40 p-3 text-sm">{status}</Card> : null}

      <Card>
        <CardTitle>Role Scope</CardTitle>
        <CardDescription>
          You are signed in as {formatRoleLabel(role)}. Standard author path: workspace edits, save commits, then create a
          changeset.
        </CardDescription>
      </Card>

      <div className="grid gap-4 lg:grid-cols-[360px_1fr]">
        <Card className="space-y-4">
          <div className="space-y-2">
            <CardTitle>Step 1: Select Workspace</CardTitle>
            <Select
              value={selectedWorkspaceId}
              onChange={(event) => {
                setSelectedWorkspaceId(event.target.value);
                setTreePath("");
                setSelectedFilePath("");
                setEditorContent("");
                setLoadedContent("");
              }}
              aria-label="Select workspace"
            >
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
          </div>

          <div className="space-y-2">
            <CardTitle>Create Workspace</CardTitle>
            <form className="space-y-2" onSubmit={(event) => void createWorkspace(event)}>
              <label className="text-xs text-muted-foreground" htmlFor="workspace-title">
                Workspace title
              </label>
              <Input
                id="workspace-title"
                value={workspaceTitle}
                onChange={(event) => setWorkspaceTitle(event.target.value)}
                placeholder="Main Workspace"
              />
              <label className="text-xs text-muted-foreground" htmlFor="workspace-branch">
                Branch (optional)
              </label>
              <Input
                id="workspace-branch"
                value={workspaceBranch}
                onChange={(event) => setWorkspaceBranch(event.target.value)}
                placeholder="feature/config-change"
              />
              <Button type="submit">Create workspace</Button>
            </form>
          </div>

          <div className="space-y-2">
            <CardTitle>Workspace Actions</CardTitle>
            <CardDescription>Keep your branch synced and checkpoint large edits.</CardDescription>
            <div className="flex flex-wrap gap-2">
              <Button
                type="button"
                variant="secondary"
                onClick={() => void runWorkspaceAction("sync-integration", "Workspace synced with integration branch.")}
                disabled={!selectedWorkspaceId}
              >
                Sync
              </Button>
              <Button
                type="button"
                variant="secondary"
                onClick={() => void runWorkspaceAction("reset", "Workspace reset to integration branch.")}
                disabled={!selectedWorkspaceId}
              >
                Reset
              </Button>
              <Button
                type="button"
                variant="secondary"
                onClick={() => void runWorkspaceAction("checkpoints", "Workspace checkpoint created.")}
                disabled={!selectedWorkspaceId}
              >
                Checkpoint
              </Button>
            </div>
          </div>

          <div className="space-y-2 rounded-lg border border-border bg-muted/20 p-3">
            <CardTitle>Step 3: Create Changeset</CardTitle>
            <CardDescription>Create a review item from the selected workspace.</CardDescription>
            <label className="text-xs text-muted-foreground" htmlFor="changeset-title">
              Changeset title
            </label>
            <Input
              id="changeset-title"
              value={changesetTitle}
              onChange={(event) => setChangesetTitle(event.target.value)}
              placeholder="Update checkout rules"
            />
            <label className="text-xs text-muted-foreground" htmlFor="changeset-description">
              Description
            </label>
            <Input
              id="changeset-description"
              value={changesetDescription}
              onChange={(event) => setChangesetDescription(event.target.value)}
              placeholder="Summarize what changed and why"
            />
            <div className="flex flex-wrap gap-2">
              <Button type="button" onClick={() => void createChangesetFromWorkspace()} disabled={!selectedWorkspaceId}>
                Create changeset
              </Button>
              <Button type="button" variant="outline" onClick={() => navigate("/changesets")}>
                Open changesets
              </Button>
            </div>
            {createdChangesetId ? (
              <p className="text-xs text-muted-foreground">Created changeset: {createdChangesetId}</p>
            ) : null}
          </div>

          <div className="space-y-2">
            <CardTitle>Step 2: File Tree</CardTitle>
            <div className="grid grid-cols-[1fr_auto] gap-2">
              <Input value={treePath} onChange={(event) => setTreePath(event.target.value)} placeholder="folder path" />
              <Button
                type="button"
                variant="outline"
                onClick={() => setTreePath(parentPath(treePath))}
                disabled={!treePath}
              >
                Up
              </Button>
            </div>
            <Input value={treeFilter} onChange={(event) => setTreeFilter(event.target.value)} placeholder="Filter files" />
            <div className="max-h-[360px] space-y-1 overflow-auto">
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
                      setLoadedContent("");
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
          </div>
        </Card>

        <Card className="space-y-3">
          <CardTitle>Editor</CardTitle>
          <CardDescription>
            {selectedFilePath
              ? `Editing ${selectedFilePath}`
              : "Select a file from the left panel to open it in the editor."}
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

              <div className="grid gap-2 lg:grid-cols-[1fr_auto]">
                <Input
                  value={commitMessage}
                  onChange={(event) => setCommitMessage(event.target.value)}
                  placeholder="commit message"
                  aria-label="Commit message"
                />
                {hasUnsavedChanges ? (
                  <span className="inline-flex items-center rounded-md bg-warning px-2 py-1 text-xs font-medium text-warning-foreground">
                    Unsaved changes
                  </span>
                ) : (
                  <span className="inline-flex items-center rounded-md bg-success/30 px-2 py-1 text-xs font-medium text-success-foreground">
                    Saved
                  </span>
                )}
              </div>

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
                <Button type="button" variant="outline" onClick={() => navigate("/changesets")}>
                  Go to changesets
                </Button>
              </div>
            </>
          ) : (
            <p className="text-sm text-muted-foreground">
              Select a file from the left panel to edit and commit workspace changes.
            </p>
          )}
        </Card>
      </div>

      <details>
        <summary className="cursor-pointer text-xs text-muted-foreground">Advanced payload</summary>
        <div className="mt-2">
          <RawDataPanel
            title="Workspace payload"
            value={{
              selectedWorkspace,
              tree: treeQuery.data,
              file: fileQuery.data,
            }}
          />
        </div>
      </details>
    </Page>
  );
}

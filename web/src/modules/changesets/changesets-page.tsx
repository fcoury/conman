import { FormEvent, useMemo, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";

import { ApiError } from "@/api/client";
import { Button } from "@/components/ui/button";
import { Card, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Select } from "@/components/ui/select";
import { Textarea } from "@/components/ui/textarea";
import { useApi } from "@/hooks/use-api";
import { useRepoContext } from "@/hooks/use-repo-context";
import { JsonView } from "@/components/ui/json-view";
import { StatusPill } from "@/components/ui/status-pill";
import { Page } from "@/modules/shared/page";
import type { Changeset, Workspace } from "@/types/api";

const reviewActions = ["approve", "request_changes", "reject"];

export function ChangesetsPage(): React.ReactElement {
  const api = useApi();
  const queryClient = useQueryClient();
  const context = useRepoContext();
  const repoId = context?.repo?.id;

  const [workspaceId, setWorkspaceId] = useState("");
  const [title, setTitle] = useState("Update config");
  const [description, setDescription] = useState("Change request from UI");
  const [selectedChangesetId, setSelectedChangesetId] = useState("");
  const [reviewAction, setReviewAction] = useState("approve");
  const [submitOverridesJson, setSubmitOverridesJson] = useState("[]");
  const [commentBody, setCommentBody] = useState("looks good");
  const [diffFormat, setDiffFormat] = useState("semantic");
  const [diffResponse, setDiffResponse] = useState<unknown>(null);
  const [commentsResponse, setCommentsResponse] = useState<unknown>(null);
  const [error, setError] = useState<string | null>(null);

  const workspacesQuery = useQuery({
    queryKey: ["changesets", "workspaces", repoId],
    queryFn: () => api.data<Workspace[]>(`/api/repos/${repoId}/workspaces`),
    enabled: Boolean(repoId),
  });

  const changesetsQuery = useQuery({
    queryKey: ["changesets", repoId],
    queryFn: async () => {
      const envelope = await api.paginated<Changeset[]>(`/api/repos/${repoId}/changesets?page=1&limit=100`);
      return envelope.data;
    },
    enabled: Boolean(repoId),
    refetchInterval: 3000,
  });

  const selectedChangeset = useMemo(
    () => changesetsQuery.data?.find((changeset) => changeset.id === selectedChangesetId) ?? null,
    [changesetsQuery.data, selectedChangesetId],
  );

  const refresh = async (): Promise<void> => {
    await queryClient.invalidateQueries({ queryKey: ["changesets", repoId] });
    await queryClient.invalidateQueries({ queryKey: ["jobs", repoId] });
  };

  const withAction = async (fn: () => Promise<void>): Promise<void> => {
    setError(null);
    try {
      await fn();
      await refresh();
    } catch (cause) {
      setError(cause instanceof ApiError ? cause.message : "changeset action failed");
    }
  };

  const createChangeset = async (event: FormEvent<HTMLFormElement>): Promise<void> => {
    event.preventDefault();
    if (!repoId || !workspaceId) return;
    await withAction(async () => {
      const created = await api.data<Changeset>(`/api/repos/${repoId}/changesets`, {
        method: "POST",
        body: JSON.stringify({ workspace_id: workspaceId, title, description }),
      });
      setSelectedChangesetId(created.id);
    });
  };

  if (!repoId) {
    return <Page title="Changesets">Bind a repo first in Setup.</Page>;
  }

  return (
    <Page title="Changesets" description="Review and advance changesets through submit/review/queue states.">
      {error ? <Card className="border-destructive/40 bg-destructive/10 p-3 text-sm">{error}</Card> : null}

      <div className="grid gap-4 lg:grid-cols-2">
        <Card>
          <CardTitle>Create Changeset</CardTitle>
          <form className="mt-3 space-y-2" onSubmit={(event) => void createChangeset(event)}>
            <Select value={workspaceId} onChange={(event) => setWorkspaceId(event.target.value)}>
              <option value="">Select workspace</option>
              {(workspacesQuery.data ?? []).map((workspace) => (
                <option key={workspace.id} value={workspace.id}>
                  {workspace.title || workspace.branch_name}
                </option>
              ))}
            </Select>
            <Input value={title} onChange={(event) => setTitle(event.target.value)} placeholder="title" required />
            <Textarea value={description} onChange={(event) => setDescription(event.target.value)} placeholder="description" />
            <Button type="submit" disabled={!workspaceId}>Create changeset</Button>
          </form>
        </Card>

        <Card>
          <CardTitle>Selected Changeset</CardTitle>
          <div className="mt-3 space-y-2">
            <Select value={selectedChangesetId} onChange={(event) => setSelectedChangesetId(event.target.value)}>
              <option value="">Select changeset</option>
              {(changesetsQuery.data ?? []).map((changeset) => (
                <option key={changeset.id} value={changeset.id}>
                  {changeset.title} ({changeset.state})
                </option>
              ))}
            </Select>
            <div className="flex flex-wrap gap-2">
              <Button
                type="button"
                variant="secondary"
                disabled={!selectedChangesetId}
                onClick={() =>
                  void withAction(async () => {
                    await api.data(`/api/repos/${repoId}/changesets/${selectedChangesetId}/submit`, {
                      method: "POST",
                      body: JSON.stringify({ profile_overrides: JSON.parse(submitOverridesJson) }),
                    });
                  })
                }
              >
                Submit
              </Button>
              <Button
                type="button"
                variant="secondary"
                disabled={!selectedChangesetId}
                onClick={() =>
                  void withAction(async () => {
                    await api.data(`/api/repos/${repoId}/changesets/${selectedChangesetId}/resubmit`, {
                      method: "POST",
                      body: JSON.stringify({ profile_overrides: JSON.parse(submitOverridesJson) }),
                    });
                  })
                }
              >
                Resubmit
              </Button>
              <Button
                type="button"
                variant="secondary"
                disabled={!selectedChangesetId}
                onClick={() =>
                  void withAction(async () => {
                    await api.data(`/api/repos/${repoId}/changesets/${selectedChangesetId}/queue`, {
                      method: "POST",
                      body: JSON.stringify({}),
                    });
                  })
                }
              >
                Queue
              </Button>
              <Button
                type="button"
                variant="danger"
                disabled={!selectedChangesetId}
                onClick={() =>
                  void withAction(async () => {
                    await api.data(`/api/repos/${repoId}/changesets/${selectedChangesetId}/move-to-draft`, {
                      method: "POST",
                      body: JSON.stringify({}),
                    });
                  })
                }
              >
                Move to draft
              </Button>
            </div>
            <Textarea
              value={submitOverridesJson}
              onChange={(event) => setSubmitOverridesJson(event.target.value)}
              className="min-h-24 font-mono"
              placeholder='[{"key":"FEATURE_X","value":{"type":"boolean","value":true}}]'
            />
          </div>
        </Card>
      </div>

      <Card className="space-y-3">
        <CardTitle>Review / Diff / Comments</CardTitle>
        <div className="grid gap-2 lg:grid-cols-4">
          <Select value={reviewAction} onChange={(event) => setReviewAction(event.target.value)}>
            {reviewActions.map((action) => (
              <option key={action} value={action}>
                {action}
              </option>
            ))}
          </Select>
          <Button
            type="button"
            variant="secondary"
            disabled={!selectedChangesetId}
            onClick={() =>
              void withAction(async () => {
                await api.data(`/api/repos/${repoId}/changesets/${selectedChangesetId}/review`, {
                  method: "POST",
                  body: JSON.stringify({ action: reviewAction }),
                });
              })
            }
          >
            Submit review
          </Button>
          <Select value={diffFormat} onChange={(event) => setDiffFormat(event.target.value)}>
            <option value="semantic">semantic</option>
            <option value="raw">raw</option>
          </Select>
          <Button
            type="button"
            variant="secondary"
            disabled={!selectedChangesetId}
            onClick={() =>
              void (async () => {
                const data = await api.data(
                  `/api/repos/${repoId}/changesets/${selectedChangesetId}/diff?format=${encodeURIComponent(diffFormat)}`,
                );
                setDiffResponse(data);
              })()
            }
          >
            Load diff
          </Button>
        </div>
        <div className="grid gap-2 lg:grid-cols-[1fr_auto]">
          <Input value={commentBody} onChange={(event) => setCommentBody(event.target.value)} placeholder="comment body" />
          <div className="flex gap-2">
            <Button
              type="button"
              variant="secondary"
              disabled={!selectedChangesetId}
              onClick={() =>
                void withAction(async () => {
                  await api.data(`/api/repos/${repoId}/changesets/${selectedChangesetId}/comments`, {
                    method: "POST",
                    body: JSON.stringify({ body: commentBody }),
                  });
                })
              }
            >
              Add comment
            </Button>
            <Button
              type="button"
              variant="secondary"
              disabled={!selectedChangesetId}
              onClick={() =>
                void (async () => {
                  const data = await api.data(`/api/repos/${repoId}/changesets/${selectedChangesetId}/comments`);
                  setCommentsResponse(data);
                })()
              }
            >
              Load comments
            </Button>
          </div>
        </div>
      </Card>

      <Card className="space-y-3">
        <CardTitle>Changesets</CardTitle>
        <div className="space-y-2">
          {(changesetsQuery.data ?? []).map((changeset) => (
            <button
              key={changeset.id}
              type="button"
              className="bg-muted hover:bg-accent flex w-full items-center justify-between rounded-md p-2"
              onClick={() => setSelectedChangesetId(changeset.id)}
            >
              <span className="text-sm">{changeset.title}</span>
              <StatusPill label={changeset.state} />
            </button>
          ))}
        </div>
        <JsonView value={selectedChangeset ?? changesetsQuery.data ?? []} />
      </Card>

      {diffResponse ? (
        <Card>
          <CardTitle>Diff</CardTitle>
          <div className="mt-3">
            <JsonView value={diffResponse} />
          </div>
        </Card>
      ) : null}

      {commentsResponse ? (
        <Card>
          <CardTitle>Comments</CardTitle>
          <div className="mt-3">
            <JsonView value={commentsResponse} />
          </div>
        </Card>
      ) : null}
    </Page>
  );
}

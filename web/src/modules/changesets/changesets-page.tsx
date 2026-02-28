import { FormEvent, useEffect, useMemo, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";

import { ApiError } from "@/api/client";
import { Button } from "@/components/ui/button";
import { Card, CardDescription, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { RawDataPanel } from "@/components/ui/raw-data-panel";
import { Select } from "@/components/ui/select";
import { StatusPill } from "@/components/ui/status-pill";
import { Textarea } from "@/components/ui/textarea";
import { useApi } from "@/hooks/use-api";
import { useRepoContext } from "@/hooks/use-repo-context";
import { canManageReleases, canReviewChangesets, formatRoleLabel } from "@/lib/rbac";
import { formatDate } from "@/lib/utils";
import {
  countChangesetsByState,
  filterChangesetsByState,
  parseOverrides,
  type ChangesetFilterState,
} from "@/modules/changesets/changesets-utils";
import { Page } from "@/modules/shared/page";
import type { Changeset, Workspace } from "@/types/api";

const reviewActions = ["approve", "request_changes", "reject"] as const;
type ReviewAction = (typeof reviewActions)[number];
const activeChangesetSignals = ["review", "approve", "queue"];

interface ChangesetsPageProps {
  mode?: "all" | "review";
}

export function ChangesetsPage({ mode = "all" }: ChangesetsPageProps): React.ReactElement {
  const api = useApi();
  const queryClient = useQueryClient();
  const context = useRepoContext();
  const repoId = context?.repo?.id;
  const role = context?.role;
  const reviewMode = mode === "review";

  const [workspaceId, setWorkspaceId] = useState("");
  const [title, setTitle] = useState("Update config");
  const [description, setDescription] = useState("Change request from UI");
  const [selectedChangesetId, setSelectedChangesetId] = useState("");
  const [filterState, setFilterState] = useState<ChangesetFilterState>(reviewMode ? "review" : "all");
  const [reviewAction, setReviewAction] = useState<ReviewAction>("approve");
  const [submitOverridesJson, setSubmitOverridesJson] = useState("[]");
  const [commentBody, setCommentBody] = useState("looks good");
  const [diffFormat, setDiffFormat] = useState("semantic");
  const [diffResponse, setDiffResponse] = useState<unknown>(null);
  const [commentsResponse, setCommentsResponse] = useState<unknown>(null);
  const [status, setStatus] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const canReview = canReviewChangesets(role);
  const canQueue = canManageReleases(role);

  useEffect(() => {
    setFilterState(reviewMode ? "review" : "all");
  }, [reviewMode]);

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
    refetchInterval: (query) => {
      const changesets = query.state.data ?? [];
      if (!changesets.length) return false;
      const hasActiveReviewFlow = changesets.some((changeset) =>
        activeChangesetSignals.some((signal) => changeset.state.toLowerCase().includes(signal)),
      );
      return hasActiveReviewFlow ? 3000 : 10000;
    },
    refetchIntervalInBackground: false,
  });

  const counts = useMemo(() => {
    return countChangesetsByState(changesetsQuery.data ?? []);
  }, [changesetsQuery.data]);

  const filteredChangesets = useMemo(() => {
    return filterChangesetsByState(changesetsQuery.data ?? [], filterState);
  }, [changesetsQuery.data, filterState]);

  useEffect(() => {
    if (!filteredChangesets.length) {
      setSelectedChangesetId("");
      return;
    }
    if (!selectedChangesetId || !filteredChangesets.some((changeset) => changeset.id === selectedChangesetId)) {
      setSelectedChangesetId(filteredChangesets[0].id);
    }
  }, [filteredChangesets, selectedChangesetId]);

  const selectedChangeset = useMemo(
    () => changesetsQuery.data?.find((changeset) => changeset.id === selectedChangesetId) ?? null,
    [changesetsQuery.data, selectedChangesetId],
  );

  const selectedState = selectedChangeset?.state.toLowerCase() ?? "";
  const isDraft = selectedState.includes("draft");
  const isQueued = selectedState.includes("queue");

  const refresh = async (): Promise<void> => {
    await queryClient.invalidateQueries({ queryKey: ["changesets", repoId] });
    await queryClient.invalidateQueries({ queryKey: ["jobs", repoId] });
  };

  const withAction = async (fn: () => Promise<void>, successMessage: string): Promise<void> => {
    setError(null);
    setStatus(null);
    try {
      await fn();
      await refresh();
      setStatus(successMessage);
    } catch (cause) {
      setError(cause instanceof ApiError ? cause.message : "changeset action failed");
    }
  };

  const createChangeset = async (event: FormEvent<HTMLFormElement>): Promise<void> => {
    event.preventDefault();
    if (!repoId || !workspaceId) return;
    await withAction(
      async () => {
        const created = await api.data<Changeset>(`/api/repos/${repoId}/changesets`, {
          method: "POST",
          body: JSON.stringify({ workspace_id: workspaceId, title, description }),
        });
        setSelectedChangesetId(created.id);
      },
      "Changeset created.",
    );
  };

  const loadDiff = async (): Promise<void> => {
    if (!repoId || !selectedChangesetId) return;
    setError(null);
    try {
      const data = await api.data(
        `/api/repos/${repoId}/changesets/${selectedChangesetId}/diff?format=${encodeURIComponent(diffFormat)}`,
      );
      setDiffResponse(data);
    } catch (cause) {
      setError(cause instanceof ApiError ? cause.message : "failed to load diff");
    }
  };

  const loadComments = async (): Promise<void> => {
    if (!repoId || !selectedChangesetId) return;
    setError(null);
    try {
      const data = await api.data(`/api/repos/${repoId}/changesets/${selectedChangesetId}/comments`);
      setCommentsResponse(data);
    } catch (cause) {
      setError(cause instanceof ApiError ? cause.message : "failed to load comments");
    }
  };

  if (!repoId) {
    return <Page title="Changesets">Bind a repo first in Setup.</Page>;
  }

  return (
    <Page
      title={reviewMode ? "Review Queue" : "Changesets"}
      description={
        reviewMode
          ? "Focus on in-review changesets, inspect semantic impact, and submit review outcomes."
          : "Submit workspace edits for review, assess semantic impact, and move approved work into release queue."
      }
    >
      {error ? <Card className="border-destructive/40 bg-destructive/10 p-3 text-sm">{error}</Card> : null}
      {status ? (
        <Card className="border-success/40 bg-success/40 p-3 text-sm" aria-live="polite">
          {status}
        </Card>
      ) : null}

      <Card>
        <CardTitle>Role Scope</CardTitle>
        <CardDescription>
          You are signed in as {formatRoleLabel(role)}.{" "}
          {reviewMode
            ? "Reviewers decide outcomes here, while config managers queue approved work."
            : "Members submit changesets, reviewers decide outcomes, and config managers queue approved changes."}
        </CardDescription>
      </Card>

      <div className="grid gap-4 xl:grid-cols-[340px_1fr]">
        <Card className="space-y-4">
          {!reviewMode ? (
            <div className="space-y-2">
              <CardTitle>Create Changeset</CardTitle>
              <CardDescription>Open a changeset from a workspace branch.</CardDescription>
              <form className="space-y-2" onSubmit={(event) => void createChangeset(event)}>
                <label className="text-xs text-muted-foreground" htmlFor="changeset-workspace-select">
                  Workspace
                </label>
                <Select
                  id="changeset-workspace-select"
                  value={workspaceId}
                  onChange={(event) => setWorkspaceId(event.target.value)}
                >
                  <option value="">Select workspace</option>
                  {(workspacesQuery.data ?? []).map((workspace) => (
                    <option key={workspace.id} value={workspace.id}>
                      {workspace.title || workspace.branch_name}
                    </option>
                  ))}
                </Select>
                <label className="text-xs text-muted-foreground" htmlFor="changeset-title-input">
                  Title
                </label>
                <Input id="changeset-title-input" value={title} onChange={(event) => setTitle(event.target.value)} required />
                <label className="text-xs text-muted-foreground" htmlFor="changeset-description-input">
                  Description
                </label>
                <Textarea
                  id="changeset-description-input"
                  value={description}
                  onChange={(event) => setDescription(event.target.value)}
                  placeholder="Describe intent and impact"
                />
                <Button type="submit" disabled={!workspaceId}>
                  Create changeset
                </Button>
              </form>
            </div>
          ) : (
            <div className="space-y-2">
              <CardTitle>Reviewer Focus</CardTitle>
              <CardDescription>Start from In Review items, then move to approved queue validation.</CardDescription>
            </div>
          )}

          <div className="space-y-2">
            <CardTitle>{reviewMode ? "Queue Filters" : "Review Queue"}</CardTitle>
            <div className="flex flex-wrap gap-2 text-xs">
              {([
                ["all", `All (${counts.all})`],
                ["draft", `Draft (${counts.draft})`],
                ["review", `In Review (${counts.review})`],
                ["approved", `Approved (${counts.approved})`],
                ["queued", `Queued (${counts.queued})`],
              ] as Array<[ChangesetFilterState, string]>).map(([key, label]) => (
                <Button
                  key={key}
                  type="button"
                  variant={filterState === key ? "primary" : "outline"}
                  size="sm"
                  onClick={() => setFilterState(key)}
                >
                  {label}
                </Button>
              ))}
            </div>
          </div>

          <div className="max-h-[460px] space-y-2 overflow-auto pr-1">
            {filteredChangesets.map((changeset) => (
              <button
                key={changeset.id}
                type="button"
                className={`w-full rounded-md border p-3 text-left transition-colors ${
                  selectedChangesetId === changeset.id
                    ? "border-primary bg-primary/10"
                    : "border-border bg-muted/30 hover:bg-accent"
                }`}
                onClick={() => setSelectedChangesetId(changeset.id)}
              >
                <div className="flex items-center justify-between gap-2">
                  <span className="truncate text-sm font-medium">{changeset.title}</span>
                  <StatusPill label={changeset.state} />
                </div>
                <p className="mt-1 text-xs text-muted-foreground">updated {formatDate(changeset.updated_at)}</p>
              </button>
            ))}
            {!filteredChangesets.length ? (
              <p className="text-sm text-muted-foreground">No changesets in this filter.</p>
            ) : null}
          </div>
        </Card>

        <Card className="space-y-4">
          <CardTitle>Changeset Detail</CardTitle>
          {!selectedChangeset ? (
            <p className="text-sm text-muted-foreground">Select or create a changeset to view details and actions.</p>
          ) : (
            <>
              <Card className="bg-muted/30 p-3">
                <div className="flex flex-wrap items-center gap-2">
                  <h3 className="text-sm font-semibold">{selectedChangeset.title}</h3>
                  <StatusPill label={selectedChangeset.state} />
                </div>
                <p className="mt-1 text-sm text-muted-foreground">
                  {selectedChangeset.description || "No description provided."}
                </p>
                <div className="mt-2 grid gap-1 text-xs text-muted-foreground lg:grid-cols-3">
                  <span>Workspace: {selectedChangeset.workspace_id}</span>
                  <span>Revision: {selectedChangeset.revision}</span>
                  <span>Updated: {formatDate(selectedChangeset.updated_at)}</span>
                </div>
              </Card>

              <Card className="space-y-3 border border-border/60 p-3">
                <CardTitle className="text-sm">Primary Actions</CardTitle>
                <div className="flex flex-wrap gap-2">
                  <Button
                    type="button"
                    variant="secondary"
                    disabled={!selectedChangesetId || !isDraft}
                    onClick={() =>
                      void withAction(
                        async () => {
                          await api.data(`/api/repos/${repoId}/changesets/${selectedChangesetId}/submit`, {
                            method: "POST",
                            body: JSON.stringify({ profile_overrides: parseOverrides(submitOverridesJson) }),
                          });
                        },
                        "Changeset submitted for review.",
                      )
                    }
                  >
                    Submit
                  </Button>
                  <Button
                    type="button"
                    variant="secondary"
                    disabled={!selectedChangesetId || isDraft || isQueued}
                    onClick={() =>
                      void withAction(
                        async () => {
                          await api.data(`/api/repos/${repoId}/changesets/${selectedChangesetId}/resubmit`, {
                            method: "POST",
                            body: JSON.stringify({ profile_overrides: parseOverrides(submitOverridesJson) }),
                          });
                        },
                        "Changeset resubmitted.",
                      )
                    }
                  >
                    Resubmit
                  </Button>
                  <Button
                    type="button"
                    variant="secondary"
                    disabled={!selectedChangesetId || !canQueue || isQueued}
                    onClick={() =>
                      void withAction(
                        async () => {
                          await api.data(`/api/repos/${repoId}/changesets/${selectedChangesetId}/queue`, {
                            method: "POST",
                            body: JSON.stringify({}),
                          });
                        },
                        "Changeset queued for release.",
                      )
                    }
                  >
                    Queue for release
                  </Button>
                  <Button
                    type="button"
                    variant="danger"
                    disabled={!selectedChangesetId || isDraft}
                    onClick={() =>
                      void withAction(
                        async () => {
                          await api.data(`/api/repos/${repoId}/changesets/${selectedChangesetId}/move-to-draft`, {
                            method: "POST",
                            body: JSON.stringify({}),
                          });
                        },
                        "Changeset moved back to draft.",
                      )
                    }
                  >
                    Move to draft
                  </Button>
                </div>
                <details>
                  <summary className="cursor-pointer text-xs text-muted-foreground">Advanced submit overrides</summary>
                  <Textarea
                    value={submitOverridesJson}
                    onChange={(event) => setSubmitOverridesJson(event.target.value)}
                    className="mt-2 min-h-24 font-mono"
                    placeholder='[{"key":"FEATURE_X","value":{"type":"boolean","value":true}}]'
                  />
                </details>
              </Card>

              <Card className="space-y-3 border border-border/60 p-3">
                <CardTitle className="text-sm">Review</CardTitle>
                <CardDescription>
                  Semantic diff should be reviewed first. Reviewers can approve, request changes, or reject.
                </CardDescription>
                <div className="grid gap-2 lg:grid-cols-[220px_1fr_auto]">
                  <Select value={reviewAction} onChange={(event) => setReviewAction(event.target.value as ReviewAction)}>
                    {reviewActions.map((action) => (
                      <option key={action} value={action}>
                        {action}
                      </option>
                    ))}
                  </Select>
                  <Input value={commentBody} onChange={(event) => setCommentBody(event.target.value)} placeholder="review note" />
                  <Button
                    type="button"
                    variant="secondary"
                    disabled={!selectedChangesetId || !canReview || isQueued}
                    onClick={() =>
                      void withAction(
                        async () => {
                          await api.data(`/api/repos/${repoId}/changesets/${selectedChangesetId}/review`, {
                            method: "POST",
                            body: JSON.stringify({ action: reviewAction }),
                          });
                          if (commentBody.trim()) {
                            await api.data(`/api/repos/${repoId}/changesets/${selectedChangesetId}/comments`, {
                              method: "POST",
                              body: JSON.stringify({ body: commentBody.trim() }),
                            });
                          }
                        },
                        "Review action submitted.",
                      )
                    }
                  >
                    Submit review
                  </Button>
                </div>
              </Card>

              <div className="grid gap-4 lg:grid-cols-2">
                <Card className="space-y-2">
                  <div className="flex items-center justify-between">
                    <CardTitle className="text-sm">Diff</CardTitle>
                    <div className="flex items-center gap-2">
                      <Select value={diffFormat} onChange={(event) => setDiffFormat(event.target.value)}>
                        <option value="semantic">semantic</option>
                        <option value="raw">raw</option>
                      </Select>
                      <Button type="button" variant="outline" onClick={() => void loadDiff()}>
                        Load diff
                      </Button>
                    </div>
                  </div>
                  {diffResponse ? (
                    <RawDataPanel title="Diff payload" value={diffResponse} />
                  ) : (
                    <p className="text-sm text-muted-foreground">Load diff to inspect file and semantic impact.</p>
                  )}
                </Card>

                <Card className="space-y-2">
                  <div className="flex items-center justify-between">
                    <CardTitle className="text-sm">Comments</CardTitle>
                    <Button type="button" variant="outline" onClick={() => void loadComments()}>
                      Load comments
                    </Button>
                  </div>
                  {commentsResponse ? (
                    <RawDataPanel title="Comments payload" value={commentsResponse} />
                  ) : (
                    <p className="text-sm text-muted-foreground">Load comments to view discussion and context.</p>
                  )}
                </Card>
              </div>
            </>
          )}
        </Card>
      </div>

      <details>
        <summary className="cursor-pointer text-xs text-muted-foreground">Advanced selected payload</summary>
        <div className="mt-2">
          <RawDataPanel title="Selected changeset payload" value={selectedChangeset ?? changesetsQuery.data ?? []} />
        </div>
      </details>
    </Page>
  );
}

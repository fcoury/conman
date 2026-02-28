import { useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";

import { Card, CardTitle } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { RawDataPanel } from "@/components/ui/raw-data-panel";
import { useApi } from "@/hooks/use-api";
import { useRepoContext } from "@/hooks/use-repo-context";
import { formatRoleLabel } from "@/lib/rbac";
import { StatusPill } from "@/components/ui/status-pill";
import { Page } from "@/modules/shared/page";
import type { Job } from "@/types/api";

const activeStates = new Set(["queued", "running"]);

export function JobsPage(): React.ReactElement {
  const api = useApi();
  const context = useRepoContext();
  const repoId = context?.repo?.id;
  const role = context?.role;
  const [selectedJobId, setSelectedJobId] = useState<string | null>(null);

  const jobsQuery = useQuery({
    queryKey: ["jobs", repoId],
    queryFn: async () => {
      const response = await api.paginated<Job[]>(`/api/repos/${repoId}/jobs?page=1&limit=100`);
      return response.data;
    },
    enabled: Boolean(repoId),
    refetchInterval: (query) => {
      const jobs = (query.state.data ?? []) as Job[];
      return jobs.some((job) => activeStates.has(job.state)) ? 2000 : false;
    },
    refetchIntervalInBackground: false,
  });

  const selectedJob = useMemo(
    () => jobsQuery.data?.find((job) => job.id === selectedJobId) ?? jobsQuery.data?.[0] ?? null,
    [jobsQuery.data, selectedJobId],
  );
  const counts = useMemo(() => {
    const summary = { total: 0, queued: 0, running: 0, failed: 0 };
    for (const job of jobsQuery.data ?? []) {
      summary.total += 1;
      const state = job.state.toLowerCase();
      if (state === "queued") summary.queued += 1;
      if (state === "running") summary.running += 1;
      if (state === "failed") summary.failed += 1;
    }
    return summary;
  }, [jobsQuery.data]);

  const jobDetailQuery = useQuery({
    queryKey: ["job", repoId, selectedJob?.id],
    queryFn: () => api.data<Job>(`/api/repos/${repoId}/jobs/${selectedJob?.id}`),
    enabled: Boolean(repoId && selectedJob?.id),
    refetchInterval: (query) => {
      const state = (query.state.data as Job | undefined)?.state;
      return state && activeStates.has(state) ? 2000 : false;
    },
    refetchIntervalInBackground: false,
  });

  if (!repoId) {
    return <Page title="Jobs">Bind a repo first in Setup.</Page>;
  }

  return (
    <Page title="Jobs" description="Track background operations triggered by changesets, releases, and deployments.">
      <Card>
        <CardTitle>Role Scope</CardTitle>
        <p className="text-sm text-muted-foreground">
          You are signed in as {formatRoleLabel(role)}. Jobs are visible to help diagnose flow status.
        </p>
      </Card>
      <Card className="space-y-2">
        <CardTitle>Queue Snapshot</CardTitle>
        <div className="flex flex-wrap gap-2 text-xs">
          <StatusPill label={`total ${counts.total}`} />
          <StatusPill label={`queued ${counts.queued}`} />
          <StatusPill label={`running ${counts.running}`} />
          <StatusPill label={`failed ${counts.failed}`} />
        </div>
      </Card>
      <div className="grid gap-4 lg:grid-cols-[360px_1fr]">
        <Card className="space-y-2">
          <CardTitle>Job List</CardTitle>
          <div className="max-h-[520px] space-y-2 overflow-auto pr-1">
            {(jobsQuery.data ?? []).map((job) => (
              <button
                key={job.id}
                type="button"
                className="bg-muted hover:bg-accent flex w-full flex-col items-start rounded-md p-2 text-left"
                onClick={() => setSelectedJobId(job.id)}
              >
                <div className="flex w-full items-center justify-between">
                  <span className="text-xs font-semibold">{job.job_type}</span>
                  <StatusPill label={job.state} />
                </div>
                <span className="text-muted-foreground mt-1 line-clamp-1 text-[11px]">{job.id}</span>
              </button>
            ))}
          </div>
          <Button type="button" variant="secondary" onClick={() => void jobsQuery.refetch()}>
            Refresh
          </Button>
        </Card>

        <Card className="space-y-3">
          <CardTitle>Job Detail</CardTitle>
          {!selectedJob ? (
            <p className="text-sm text-muted-foreground">Select a job to inspect details.</p>
          ) : (
            <>
              <div className="rounded-md border border-border bg-muted/30 p-3">
                <div className="flex items-center justify-between gap-2">
                  <p className="text-sm font-semibold">{selectedJob.job_type}</p>
                  <StatusPill label={selectedJob.state} />
                </div>
                <p className="mt-1 text-xs text-muted-foreground">{selectedJob.id}</p>
                <p className="mt-1 text-xs text-muted-foreground">
                  retry {selectedJob.retry_count} of {selectedJob.max_retries}
                </p>
              </div>
              <details>
                <summary className="cursor-pointer text-xs text-muted-foreground">Advanced job payload</summary>
                <div className="mt-2">
                  <RawDataPanel title="Job detail payload" value={jobDetailQuery.data ?? selectedJob} />
                </div>
              </details>
            </>
          )}
        </Card>
      </div>
    </Page>
  );
}

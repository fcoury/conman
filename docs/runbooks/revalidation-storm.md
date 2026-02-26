# Revalidation Storm

## Trigger
- Queue depth spikes after release publish.
- Revalidation jobs (`revalidate_queued_changeset`) saturate workers.

## Impact
- Slow feedback loop for queued changesets.
- Higher latency across async workflows.

## Diagnosis
1. Inspect `conman_job_queue_depth` and `conman_jobs_completed_total`.
2. Check if most jobs are `revalidate_queued_changeset`.
3. Sample recent failures for repeated override collisions.

## Resolution
1. Confirm job runner is healthy and processing ticks.
2. Temporarily pause new release publishes if queue depth keeps climbing.
3. Prioritize conflicted changesets and return them to authors quickly.
4. Resume normal publishing once queue drain trend is stable.

## Prevention
- Publish smaller release subsets.
- Rebase/revalidate high-risk queued changesets earlier.
- Alert on queue depth trend, not only absolute threshold.

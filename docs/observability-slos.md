# Observability and SLO Baseline

## Core Metrics
- `conman_http_requests_total`
- `conman_http_request_duration_seconds`
- `conman_job_queue_depth`
- `conman_jobs_enqueued_total`
- `conman_jobs_completed_total`
- `conman_job_duration_seconds`
- `conman_deployments_total`
- `conman_auth_failures_total`

## SLO Targets (initial)
1. API availability: 99.9% monthly for authenticated endpoints.
2. API latency: p95 < 500ms for read endpoints, p95 < 1500ms for write endpoints.
3. Async pipeline latency: p95 job completion < 60s for non-deploy jobs.
4. Deployment success rate: >= 99% of deployments reach terminal success.

## Alerts (initial)
1. Job queue depth sustained > 250 for 10 minutes.
2. Deploy failure ratio > 5% over 15 minutes.
3. Auth failure spike > 3x baseline over 10 minutes.
4. API 5xx rate > 1% over 5 minutes.

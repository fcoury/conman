use crate::{
    error::ApiConmanError,
    events::{emit_audit, emit_notification},
    extractors::Pagination,
    response::ApiResponse,
    state::AppState,
};
use axum::{
    Extension, Json,
    extract::{Path, Query, State},
};
use conman_auth::AuthUser;
use conman_core::{
    ConmanError, GitRepo, GitUser, Job, JobState, JobType, RefUpdate, ReleaseBatch, ReleaseState,
    Role,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
pub struct SetReleaseChangesetsRequest {
    pub changeset_ids: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct AssembleReleaseResponse {
    pub release: ReleaseBatch,
    pub job: Job,
}

#[derive(Debug, Serialize)]
pub struct PublishReleaseResponse {
    pub release: ReleaseBatch,
    pub released_changesets: Vec<String>,
}

fn git_repo(repo_id: &str, repo_path: &str) -> GitRepo {
    GitRepo {
        storage_name: "default".to_string(),
        relative_path: repo_path.to_string(),
        gl_repository: format!("project-{repo_id}"),
    }
}

fn git_user(auth: &AuthUser) -> GitUser {
    GitUser {
        gl_id: format!("user-{}", auth.user_id),
        name: auth.email.clone(),
        email: auth.email.clone(),
        gl_username: auth.email.clone(),
        timezone: "UTC".to_string(),
    }
}

pub async fn list_releases(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path(repo_id): Path<String>,
    Query(pagination): Query<Pagination>,
) -> Result<Json<ApiResponse<Vec<ReleaseBatch>>>, ApiConmanError> {
    auth.require_role(&repo_id, Role::Member)?;
    let pagination = pagination.validate()?;
    let (rows, total) = conman_db::ReleaseRepo::new(state.db.clone())
        .list_by_repo(&repo_id, pagination.skip(), pagination.limit)
        .await?;
    Ok(Json(ApiResponse::paginated(
        rows,
        pagination.page,
        pagination.limit,
        total,
    )))
}

pub async fn create_release(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path(repo_id): Path<String>,
) -> Result<Json<ApiResponse<ReleaseBatch>>, ApiConmanError> {
    auth.require_role(&repo_id, Role::ConfigManager)?;
    let repo = conman_db::ReleaseRepo::new(state.db.clone());
    let tag = repo.next_tag(&repo_id).await?;
    let release = repo.create_draft(&repo_id, tag).await?;
    if let Err(err) = emit_audit(
        &state,
        Some(&auth.user_id),
        Some(&repo_id),
        "release",
        &release.id,
        "created",
        None,
        serde_json::to_value(&release).ok(),
        None,
    )
    .await
    {
        tracing::warn!(error = %err, "failed to write audit event");
    }
    Ok(Json(ApiResponse::ok(release)))
}

pub async fn get_release(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path((repo_id, release_id)): Path<(String, String)>,
) -> Result<Json<ApiResponse<ReleaseBatch>>, ApiConmanError> {
    auth.require_role(&repo_id, Role::Member)?;
    let release = conman_db::ReleaseRepo::new(state.db.clone())
        .find_by_id(&release_id)
        .await?
        .ok_or_else(|| ConmanError::NotFound {
            entity: "release",
            id: release_id.clone(),
        })?;
    if release.repo_id != repo_id {
        return Err(ConmanError::Forbidden {
            message: "release does not belong to app".to_string(),
        }
        .into());
    }
    Ok(Json(ApiResponse::ok(release)))
}

pub async fn set_release_changesets(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path((repo_id, release_id)): Path<(String, String)>,
    Json(req): Json<SetReleaseChangesetsRequest>,
) -> Result<Json<ApiResponse<ReleaseBatch>>, ApiConmanError> {
    auth.require_role(&repo_id, Role::ConfigManager)?;

    let queued = conman_db::ChangesetRepo::new(state.db.clone())
        .list_queued_by_repo(&repo_id)
        .await?;
    let queued_ids = queued.into_iter().map(|c| c.id).collect::<Vec<_>>();
    for id in &req.changeset_ids {
        if !queued_ids.contains(id) {
            return Err(ConmanError::Conflict {
                message: format!("changeset {id} is not queued for this app"),
            }
            .into());
        }
    }

    let release = conman_db::ReleaseRepo::new(state.db.clone())
        .set_changesets(&release_id, &req.changeset_ids)
        .await?;
    if let Err(err) = emit_audit(
        &state,
        Some(&auth.user_id),
        Some(&repo_id),
        "release",
        &release.id,
        "changesets_set",
        None,
        serde_json::to_value(&release).ok(),
        None,
    )
    .await
    {
        tracing::warn!(error = %err, "failed to write audit event");
    }
    Ok(Json(ApiResponse::ok(release)))
}

pub async fn reorder_release_changesets(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path((repo_id, release_id)): Path<(String, String)>,
    Json(req): Json<SetReleaseChangesetsRequest>,
) -> Result<Json<ApiResponse<ReleaseBatch>>, ApiConmanError> {
    set_release_changesets(
        State(state),
        Extension(auth),
        Path((repo_id, release_id)),
        Json(req),
    )
    .await
}

pub async fn assemble_release(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path((repo_id, release_id)): Path<(String, String)>,
) -> Result<Json<ApiResponse<AssembleReleaseResponse>>, ApiConmanError> {
    auth.require_role(&repo_id, Role::ConfigManager)?;
    let release_repo = conman_db::ReleaseRepo::new(state.db.clone());
    let release =
        release_repo
            .find_by_id(&release_id)
            .await?
            .ok_or_else(|| ConmanError::NotFound {
                entity: "release",
                id: release_id.clone(),
            })?;
    if !matches!(
        release.state,
        ReleaseState::DraftRelease | ReleaseState::Assembling
    ) {
        return Err(ConmanError::Conflict {
            message: "release can only be assembled from draft_release/assembling".to_string(),
        }
        .into());
    }
    if release.ordered_changeset_ids.is_empty() {
        return Err(ConmanError::Validation {
            message: "release must include at least one queued changeset".to_string(),
        }
        .into());
    }

    let job = conman_db::JobRepo::new(state.db.clone())
        .enqueue(conman_db::EnqueueJobInput {
            repo_id: repo_id.clone(),
            job_type: JobType::ReleaseAssemble,
            entity_type: "release".to_string(),
            entity_id: release_id.clone(),
            payload: serde_json::json!({"release_id": release_id, "changeset_ids": release.ordered_changeset_ids}),
            max_retries: 1,
            timeout_ms: 20 * 60 * 1000,
            created_by: Some(auth.user_id.clone()),
        })
        .await?;

    let release = release_repo
        .set_state(&release_id, ReleaseState::Assembling)
        .await?;
    let release = release_repo.set_compose_job(&release.id, &job.id).await?;
    if let Err(err) = emit_audit(
        &state,
        Some(&auth.user_id),
        Some(&repo_id),
        "release",
        &release.id,
        "assemble_started",
        None,
        serde_json::to_value(&release).ok(),
        None,
    )
    .await
    {
        tracing::warn!(error = %err, "failed to write audit event");
    }

    Ok(Json(ApiResponse::ok(AssembleReleaseResponse {
        release,
        job,
    })))
}

pub async fn publish_release(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path((repo_id, release_id)): Path<(String, String)>,
) -> Result<Json<ApiResponse<PublishReleaseResponse>>, ApiConmanError> {
    auth.require_role(&repo_id, Role::ConfigManager)?;
    let repo = conman_db::ReleaseRepo::new(state.db.clone());
    let release = repo
        .find_by_id(&release_id)
        .await?
        .ok_or_else(|| ConmanError::NotFound {
            entity: "release",
            id: release_id.clone(),
        })?;
    if !matches!(
        release.state,
        ReleaseState::Assembling | ReleaseState::Validated
    ) {
        return Err(ConmanError::Conflict {
            message: "release can only be published from assembling/validated".to_string(),
        }
        .into());
    }

    let compose_job_id = release
        .compose_job_id
        .clone()
        .ok_or_else(|| ConmanError::Conflict {
            message: "release has no compose job; call assemble first".to_string(),
        })?;
    let compose_job = conman_db::JobRepo::new(state.db.clone())
        .get(&compose_job_id)
        .await?
        .ok_or_else(|| ConmanError::NotFound {
            entity: "job",
            id: compose_job_id.clone(),
        })?;
    match compose_job.state {
        JobState::Succeeded => {
            if release.state != ReleaseState::Validated {
                repo.set_state(&release.id, ReleaseState::Validated).await?;
            }
        }
        JobState::Failed | JobState::Canceled => {
            return Err(ConmanError::Conflict {
                message: format!(
                    "release assemble job is {:?}; cannot publish",
                    compose_job.state
                ),
            }
            .into());
        }
        _ => {
            return Err(ConmanError::Conflict {
                message: "release assemble job is still running; retry publish after completion"
                    .to_string(),
            }
            .into());
        }
    }

    let merge_gate = conman_db::JobRepo::new(state.db.clone())
        .latest_for_entity(&repo_id, "release", &release_id, JobType::MsuiteMerge)
        .await?;
    match merge_gate {
        Some(job) if job.state == JobState::Succeeded => {}
        Some(job) if matches!(job.state, JobState::Queued | JobState::Running) => {
            return Err(ConmanError::Conflict {
                message: format!(
                    "release merge gate in progress (job {}); retry publish",
                    job.id
                ),
            }
            .into());
        }
        Some(_) => {
            return Err(ConmanError::Conflict {
                message: "release merge gate failed; re-run assemble or fix validation issues"
                    .to_string(),
            }
            .into());
        }
        None => {
            let job = conman_db::JobRepo::new(state.db.clone())
                .enqueue(conman_db::EnqueueJobInput {
                    repo_id: repo_id.clone(),
                    job_type: JobType::MsuiteMerge,
                    entity_type: "release".to_string(),
                    entity_id: release_id.clone(),
                    payload: serde_json::json!({
                        "gate": "release_publish",
                        "release_id": release_id,
                        "repo_id": repo_id,
                    }),
                    max_retries: 1,
                    timeout_ms: 20 * 60 * 1000,
                    created_by: Some(auth.user_id.clone()),
                })
                .await?;
            return Err(ConmanError::Conflict {
                message: format!(
                    "release merge gate job enqueued ({}); publish after it succeeds",
                    job.id
                ),
            }
            .into());
        }
    }

    let app = conman_db::RepoStore::new(state.db.clone())
        .find_by_id(&repo_id)
        .await?
        .ok_or_else(|| ConmanError::NotFound {
            entity: "app",
            id: repo_id.clone(),
        })?;
    let git_repo = git_repo(&repo_id, &app.repo_path);
    let git_user = git_user(&auth);
    let integration_branch = app.integration_branch.clone();
    let integration_ref = format!("refs/heads/{integration_branch}");
    let compose_ref = format!("refs/heads/conman/release/{}", release.id);
    let base_branch = state
        .git_adapter
        .find_branch(&git_repo, &integration_branch)
        .await?
        .ok_or_else(|| ConmanError::Conflict {
            message: format!("integration branch {integration_branch} not found"),
        })?;
    let base_sha = base_branch.commit.id;

    state
        .git_adapter
        .update_references(
            &git_repo,
            vec![RefUpdate {
                reference: compose_ref.clone(),
                old_object_id: String::new(),
                new_object_id: base_sha.clone(),
            }],
        )
        .await?;

    let changeset_repo = conman_db::ChangesetRepo::new(state.db.clone());
    let mut compose_head = base_sha.clone();
    for changeset_id in &release.ordered_changeset_ids {
        let changeset = changeset_repo
            .find_by_id(changeset_id)
            .await?
            .ok_or_else(|| ConmanError::NotFound {
                entity: "changeset",
                id: changeset_id.clone(),
            })?;
        if changeset.state != conman_core::ChangesetState::Queued {
            return Err(ConmanError::Conflict {
                message: format!(
                    "changeset {} is no longer queued; refresh release selection",
                    changeset.id
                ),
            }
            .into());
        }
        let source_sha = changeset
            .submitted_head_sha
            .clone()
            .unwrap_or(changeset.head_sha.clone());
        let merge_message = format!(
            "Release {} includes changeset {} ({})",
            release.tag, changeset.id, changeset.title
        );
        match state
            .git_adapter
            .merge_to_ref(
                &git_repo,
                &git_user,
                &source_sha,
                &compose_ref,
                &compose_ref,
                &merge_message,
            )
            .await
        {
            Ok(sha) => compose_head = sha,
            Err(err) => {
                if let ConmanError::Git { message } = &err
                    && message.to_ascii_lowercase().contains("conflict")
                {
                    let _ = changeset_repo.mark_conflicted(&changeset.id).await;
                    return Err(ConmanError::Conflict {
                        message: format!(
                            "changeset {} conflicts during release compose and was moved to conflicted",
                            changeset.id
                        ),
                    }
                    .into());
                }
                return Err(err.into());
            }
        }
    }

    state
        .git_adapter
        .update_references(
            &git_repo,
            vec![RefUpdate {
                reference: integration_ref.clone(),
                old_object_id: base_sha,
                new_object_id: compose_head.clone(),
            }],
        )
        .await
        .map_err(|err| match err {
            ConmanError::Git { message } => ConmanError::Conflict {
                message: format!(
                    "integration branch moved while publishing release; re-run compose: {message}"
                ),
            },
            other => other,
        })?;

    let _ = state
        .git_adapter
        .create_tag(
            &git_repo,
            &git_user,
            &release.tag,
            &compose_head,
            &format!("Release {}", release.tag),
        )
        .await?;
    let _ = state
        .git_adapter
        .delete_branch(
            &git_repo,
            &git_user,
            compose_ref.trim_start_matches("refs/heads/"),
        )
        .await;

    let published_sha = compose_head;
    let release = repo
        .publish(&release_id, published_sha, &auth.user_id)
        .await?;
    changeset_repo
        .mark_released_batch(&release.ordered_changeset_ids)
        .await?;
    let job_repo = conman_db::JobRepo::new(state.db.clone());
    for queued in changeset_repo.list_queued_by_repo(&repo_id).await? {
        let existing = job_repo
            .latest_for_entity(
                &repo_id,
                "changeset",
                &queued.id,
                JobType::RevalidateQueuedChangeset,
            )
            .await?;
        let has_inflight = existing
            .map(|job| matches!(job.state, JobState::Queued | JobState::Running))
            .unwrap_or(false);
        if has_inflight {
            continue;
        }
        job_repo
            .enqueue(conman_db::EnqueueJobInput {
                repo_id: repo_id.clone(),
                job_type: JobType::RevalidateQueuedChangeset,
                entity_type: "changeset".to_string(),
                entity_id: queued.id,
                payload: serde_json::json!({
                    "trigger": "post_release_publish",
                    "release_id": release.id.clone(),
                }),
                max_retries: 1,
                timeout_ms: 10 * 60 * 1000,
                created_by: Some(auth.user_id.clone()),
            })
            .await?;
    }
    if let Err(err) = emit_audit(
        &state,
        Some(&auth.user_id),
        Some(&repo_id),
        "release",
        &release.id,
        "published",
        None,
        serde_json::to_value(&release).ok(),
        release.published_sha.as_deref(),
    )
    .await
    {
        tracing::warn!(error = %err, "failed to write audit event");
    }
    if let Err(err) = emit_notification(
        &state,
        &auth.user_id,
        Some(&repo_id),
        "release_published",
        "Release published",
        &format!("Release {} was published.", release.tag),
    )
    .await
    {
        tracing::warn!(error = %err, "failed to enqueue notification");
    }
    Ok(Json(ApiResponse::ok(PublishReleaseResponse {
        released_changesets: release.ordered_changeset_ids.clone(),
        release,
    })))
}

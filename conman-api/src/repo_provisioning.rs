use conman_auth::AuthUser;
use conman_core::{ConmanError, FileAction, GitRepo, GitUser};

use crate::{error::ApiConmanError, state::AppState};

#[derive(Debug, Clone)]
pub struct RepoProvisioningResult {
    pub git_repo: GitRepo,
    pub created_repo: bool,
}

fn git_repo(repo_path: &str) -> GitRepo {
    GitRepo {
        storage_name: "default".to_string(),
        relative_path: repo_path.to_string(),
        gl_repository: String::new(),
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

fn preferred_base_branch_name(branches: &[String]) -> Option<&str> {
    branches
        .iter()
        .find(|name| name.as_str() == "main")
        .or_else(|| branches.first())
        .map(String::as_str)
}

fn is_missing_parent_commit_error(err: &ConmanError) -> bool {
    matches!(
        err,
        ConmanError::Git { message }
            if message.contains("could not resolve parent commit from start_sha, start_branch_name, or branch_name")
    )
}

pub async fn ensure_repo_provisioned(
    state: &AppState,
    auth: &AuthUser,
    repo_path: &str,
    integration_branch: &str,
    instance_name: &str,
) -> Result<RepoProvisioningResult, ApiConmanError> {
    let git_repo = git_repo(repo_path);
    let git_user = git_user(auth);

    let mut created_repo = false;
    if !state.git_adapter.repo_exists(&git_repo).await? {
        state
            .git_adapter
            .create_repo(&git_repo.storage_name, &git_repo.relative_path)
            .await?;
        created_repo = true;
    }

    if state
        .git_adapter
        .find_branch(&git_repo, integration_branch)
        .await?
        .is_none()
    {
        let base_branch = preferred_base_branch_name(
            &state
                .git_adapter
                .list_branches(&git_repo)
                .await?
                .into_iter()
                .map(|branch| branch.name)
                .collect::<Vec<_>>(),
        )
        .map(str::to_string);

        if let Some(base_branch) = base_branch {
            state
                .git_adapter
                .create_branch(&git_repo, &git_user, integration_branch, &base_branch)
                .await?;
        } else {
            let initial_readme = format!("# {}\n", instance_name.trim());
            match state
                .git_adapter
                .commit_files(
                    &git_repo,
                    &git_user,
                    integration_branch,
                    None,
                    "Initialize repository",
                    vec![FileAction::Create {
                        path: "README.md".to_string(),
                        content: initial_readme.into_bytes(),
                    }],
                )
                .await
            {
                Ok(_) => {}
                Err(err) if is_missing_parent_commit_error(&err) => {
                    tracing::warn!(
                        repo_path = repo_path,
                        branch = integration_branch,
                        "repository has no base commit; leaving integration branch uninitialized"
                    );
                }
                Err(err) => return Err(err.into()),
            }
        }
    }

    Ok(RepoProvisioningResult {
        git_repo,
        created_repo,
    })
}

pub async fn cleanup_created_repo(
    state: &AppState,
    provisioning: &RepoProvisioningResult,
) -> Result<(), ConmanError> {
    if provisioning.created_repo {
        state
            .git_adapter
            .remove_repo(&provisioning.git_repo)
            .await?;
    }
    Ok(())
}

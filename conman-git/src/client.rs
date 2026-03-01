use async_trait::async_trait;
use chrono::{DateTime, TimeZone, Utc};
use tonic::{Code, Status, transport::Channel};

use conman_core::{
    CommitResult, ConmanError, FileAction, GitAuthor, GitBranch, GitCommit, GitDiffEntry,
    GitDiffStat, GitRepo, GitTag, GitTreeEntry, GitTreeEntryType, GitUser, MergeResult, RefUpdate,
    RevertResult,
};
use gitaly_proto::gitaly::{
    Branch, CommitAuthor, CommitDiffRequest, CommitIsAncestorRequest, CreateRepositoryRequest,
    DiffStatsRequest, FindAllTagsRequest, FindBranchRequest, FindCommitRequest,
    FindLocalBranchesRequest, FindTagRequest, GetBlobsRequest, GetTreeEntriesRequest,
    GitCommit as ProtoGitCommit, ListCommitsRequest, OperationBranchUpdate, RawDiffRequest,
    RemoveRepositoryRequest, Repository, RepositoryExistsRequest, Tag, UpdateReferencesRequest,
    User, UserCommitFilesAction, UserCommitFilesActionHeader, UserCommitFilesRequest,
    UserCommitFilesRequestHeader, UserCreateBranchRequest, UserCreateTagRequest,
    UserDeleteBranchRequest, UserDeleteTagRequest, UserMergeBranchRequest, UserMergeToRefRequest,
    UserRebaseToRefRequest, UserRevertRequest, blob_service_client::BlobServiceClient,
    commit_diff_request::DiffMode, commit_service_client::CommitServiceClient,
    diff_service_client::DiffServiceClient, get_blobs_request::RevisionPath,
    list_commits_request::Order, operation_service_client::OperationServiceClient,
    ref_service_client::RefServiceClient, repository_service_client::RepositoryServiceClient,
    tree_entry::EntryType, update_references_request,
    user_commit_files_action::UserCommitFilesActionPayload,
    user_commit_files_action_header::ActionType,
    user_commit_files_request::UserCommitFilesRequestPayload,
};
use tokio_stream::iter;

use crate::adapter::GitAdapter;

const ZERO_OID: &str = "0000000000000000000000000000000000000000";

#[derive(Clone)]
pub struct GitalyClient {
    channel: Channel,
}

impl GitalyClient {
    pub async fn connect(address: &str) -> Result<Self, ConmanError> {
        let channel = Channel::from_shared(address.to_string())
            .map_err(|e| ConmanError::Git {
                message: format!("invalid gitaly address: {e}"),
            })?
            .connect()
            .await
            .map_err(|e| ConmanError::Git {
                message: format!("failed to connect to gitaly: {e}"),
            })?;

        Ok(Self { channel })
    }

    pub fn channel(&self) -> &Channel {
        &self.channel
    }

    fn repo_client(&self) -> RepositoryServiceClient<Channel> {
        RepositoryServiceClient::new(self.channel.clone())
    }

    fn ref_client(&self) -> RefServiceClient<Channel> {
        RefServiceClient::new(self.channel.clone())
    }

    fn operation_client(&self) -> OperationServiceClient<Channel> {
        OperationServiceClient::new(self.channel.clone())
    }

    fn commit_client(&self) -> CommitServiceClient<Channel> {
        CommitServiceClient::new(self.channel.clone())
    }

    fn diff_client(&self) -> DiffServiceClient<Channel> {
        DiffServiceClient::new(self.channel.clone())
    }

    fn blob_client(&self) -> BlobServiceClient<Channel> {
        BlobServiceClient::new(self.channel.clone())
    }
}

fn to_proto_repo(repo: &GitRepo) -> Repository {
    Repository {
        storage_name: repo.storage_name.clone(),
        relative_path: repo.relative_path.clone(),
        gl_repository: repo.gl_repository.clone(),
        ..Default::default()
    }
}

fn to_proto_user(user: &GitUser) -> User {
    User {
        gl_id: user.gl_id.clone(),
        name: user.name.as_bytes().to_vec(),
        email: user.email.as_bytes().to_vec(),
        gl_username: user.gl_username.clone(),
        timezone: user.timezone.clone(),
    }
}

fn bytes_to_string(bytes: Vec<u8>) -> String {
    String::from_utf8_lossy(&bytes).into_owned()
}

fn status_is_not_found(status: &Status) -> bool {
    let msg = status.message().to_ascii_lowercase();
    status.code() == tonic::Code::NotFound
        || msg.contains("not found")
        || msg.contains("reference not found")
}

fn map_status_to_error(status: Status) -> ConmanError {
    ConmanError::Git {
        message: format!("gRPC {:?}: {}", status.code(), status.message()),
    }
}

fn timestamp_to_utc(ts: Option<prost_types::Timestamp>) -> DateTime<Utc> {
    match ts {
        Some(ts) => Utc
            .timestamp_opt(ts.seconds, ts.nanos as u32)
            .single()
            .unwrap_or_else(Utc::now),
        None => Utc::now(),
    }
}

fn author_from_proto(author: Option<CommitAuthor>) -> GitAuthor {
    match author {
        Some(author) => GitAuthor {
            name: bytes_to_string(author.name),
            email: bytes_to_string(author.email),
            date: timestamp_to_utc(author.date),
        },
        None => GitAuthor {
            name: String::new(),
            email: String::new(),
            date: Utc::now(),
        },
    }
}

fn commit_from_proto(commit: ProtoGitCommit) -> GitCommit {
    GitCommit {
        id: commit.id,
        subject: bytes_to_string(commit.subject),
        body: bytes_to_string(commit.body),
        author: author_from_proto(commit.author),
        committer: author_from_proto(commit.committer),
        parent_ids: commit.parent_ids,
        tree_id: commit.tree_id,
    }
}

fn branch_from_proto(branch: Branch) -> GitBranch {
    GitBranch {
        name: bytes_to_string(branch.name),
        commit: branch
            .target_commit
            .map(commit_from_proto)
            .unwrap_or_else(|| GitCommit::placeholder("")),
    }
}

fn tag_from_proto(tag: Tag) -> GitTag {
    GitTag {
        name: bytes_to_string(tag.name),
        id: tag.id,
        target_commit: tag.target_commit.map(commit_from_proto),
        message: if tag.message.is_empty() {
            None
        } else {
            Some(bytes_to_string(tag.message))
        },
        tagger: tag.tagger.map(|author| author_from_proto(Some(author))),
    }
}

fn tree_entry_type_from_proto(entry_type: i32) -> GitTreeEntryType {
    match EntryType::try_from(entry_type).unwrap_or(EntryType::Blob) {
        EntryType::Blob => GitTreeEntryType::Blob,
        EntryType::Tree => GitTreeEntryType::Tree,
        EntryType::Commit => GitTreeEntryType::Commit,
    }
}

fn commit_id_from_branch_update(update: Option<OperationBranchUpdate>) -> Option<String> {
    update.and_then(|u| (!u.commit_id.is_empty()).then_some(u.commit_id))
}

#[async_trait]
impl GitAdapter for GitalyClient {
    async fn create_repo(&self, storage: &str, path: &str) -> Result<(), ConmanError> {
        let mut client = self.repo_client();
        let request = CreateRepositoryRequest {
            repository: Some(Repository {
                storage_name: storage.to_string(),
                relative_path: path.to_string(),
                ..Default::default()
            }),
            ..Default::default()
        };

        client
            .create_repository(request)
            .await
            .map_err(map_status_to_error)?;
        Ok(())
    }

    async fn repo_exists(&self, repo: &GitRepo) -> Result<bool, ConmanError> {
        let mut client = self.repo_client();
        let request = RepositoryExistsRequest {
            repository: Some(to_proto_repo(repo)),
        };

        let response = client
            .repository_exists(request)
            .await
            .map_err(map_status_to_error)?;
        Ok(response.into_inner().exists)
    }

    async fn remove_repo(&self, repo: &GitRepo) -> Result<(), ConmanError> {
        let mut client = self.repo_client();
        let request = RemoveRepositoryRequest {
            repository: Some(to_proto_repo(repo)),
        };

        client
            .remove_repository(request)
            .await
            .map_err(map_status_to_error)?;
        Ok(())
    }

    async fn create_branch(
        &self,
        repo: &GitRepo,
        user: &GitUser,
        branch_name: &str,
        start_point: &str,
    ) -> Result<GitBranch, ConmanError> {
        let mut client = self.operation_client();
        let request = UserCreateBranchRequest {
            repository: Some(to_proto_repo(repo)),
            branch_name: branch_name.as_bytes().to_vec(),
            user: Some(to_proto_user(user)),
            start_point: start_point.as_bytes().to_vec(),
        };

        let response = client
            .user_create_branch(request)
            .await
            .map_err(map_status_to_error)?;

        if let Some(branch) = response.into_inner().branch {
            return Ok(branch_from_proto(branch));
        }

        // Some gitaly implementations return an empty create response while still creating
        // the branch. Resolve the branch explicitly before failing.
        if let Some(branch) = self.find_branch(repo, branch_name).await? {
            return Ok(branch);
        }

        // Some servers still apply the ref update even if branch lookup lags. Synthesize
        // a branch response from the requested start point to keep workspace creation
        // resilient.
        if let Some(commit) = self.find_commit(repo, start_point).await? {
            return Ok(GitBranch {
                name: branch_name.to_string(),
                commit,
            });
        }

        Err(ConmanError::Git {
            message: "create branch returned empty branch".to_string(),
        })
    }

    async fn delete_branch(
        &self,
        repo: &GitRepo,
        user: &GitUser,
        branch_name: &str,
    ) -> Result<(), ConmanError> {
        let mut client = self.operation_client();
        let request = UserDeleteBranchRequest {
            repository: Some(to_proto_repo(repo)),
            branch_name: branch_name.as_bytes().to_vec(),
            user: Some(to_proto_user(user)),
            ..Default::default()
        };

        client
            .user_delete_branch(request)
            .await
            .map_err(map_status_to_error)?;
        Ok(())
    }

    async fn find_branch(
        &self,
        repo: &GitRepo,
        name: &str,
    ) -> Result<Option<GitBranch>, ConmanError> {
        let mut client = self.ref_client();
        let request = FindBranchRequest {
            repository: Some(to_proto_repo(repo)),
            name: name.as_bytes().to_vec(),
        };

        match client.find_branch(request).await {
            Ok(response) => Ok(response.into_inner().branch.map(branch_from_proto)),
            Err(status) if status_is_not_found(&status) => Ok(None),
            Err(status) => Err(map_status_to_error(status)),
        }
    }

    async fn list_branches(&self, repo: &GitRepo) -> Result<Vec<GitBranch>, ConmanError> {
        let mut client = self.ref_client();
        let request = FindLocalBranchesRequest {
            repository: Some(to_proto_repo(repo)),
            ..Default::default()
        };

        let response = client
            .find_local_branches(request)
            .await
            .map_err(map_status_to_error)?;

        let mut stream = response.into_inner();
        let mut branches = Vec::new();
        while let Some(response) = stream.message().await.map_err(map_status_to_error)? {
            branches.extend(response.local_branches.into_iter().map(branch_from_proto));
        }

        Ok(branches)
    }

    async fn get_tree_entries(
        &self,
        repo: &GitRepo,
        revision: &str,
        path: &str,
        recursive: bool,
    ) -> Result<Vec<GitTreeEntry>, ConmanError> {
        let mut client = self.commit_client();
        let request = GetTreeEntriesRequest {
            repository: Some(to_proto_repo(repo)),
            revision: revision.as_bytes().to_vec(),
            path: path.as_bytes().to_vec(),
            recursive,
            ..Default::default()
        };

        let response = client
            .get_tree_entries(request)
            .await
            .map_err(map_status_to_error)?;

        let mut stream = response.into_inner();
        let mut entries = Vec::new();
        while let Some(response) = stream.message().await.map_err(map_status_to_error)? {
            entries.extend(response.entries.into_iter().map(|entry| GitTreeEntry {
                oid: entry.oid,
                path: bytes_to_string(entry.path),
                entry_type: tree_entry_type_from_proto(entry.r#type),
                mode: entry.mode,
                flat_path: bytes_to_string(entry.flat_path),
            }));
        }

        Ok(entries)
    }

    async fn get_blob(
        &self,
        repo: &GitRepo,
        revision: &str,
        path: &str,
    ) -> Result<Vec<u8>, ConmanError> {
        let mut client = self.blob_client();
        let request = GetBlobsRequest {
            repository: Some(to_proto_repo(repo)),
            revision_paths: vec![RevisionPath {
                revision: revision.to_string(),
                path: path.as_bytes().to_vec(),
            }],
            limit: -1,
        };

        let response = client
            .get_blobs(request)
            .await
            .map_err(map_status_to_error)?;
        let mut stream = response.into_inner();
        let mut data = Vec::new();
        let mut found_blob = false;

        while let Some(response) = stream.message().await.map_err(map_status_to_error)? {
            if !response.oid.is_empty() {
                found_blob = true;
            }
            if !response.data.is_empty() {
                data.extend(response.data);
                found_blob = true;
            }
        }

        if !found_blob {
            return Err(ConmanError::NotFound {
                entity: "blob",
                id: format!("{}:{revision}:{path}", repo.relative_path),
            });
        }

        Ok(data)
    }

    async fn commit_files(
        &self,
        repo: &GitRepo,
        user: &GitUser,
        branch_name: &str,
        start_branch_name: Option<&str>,
        message: &str,
        actions: Vec<FileAction>,
    ) -> Result<CommitResult, ConmanError> {
        let mut client = self.operation_client();

        let build_requests = |expected_old_oid: Option<&str>| {
            let mut requests = Vec::new();
            requests.push(UserCommitFilesRequest {
                user_commit_files_request_payload: Some(UserCommitFilesRequestPayload::Header(
                    UserCommitFilesRequestHeader {
                        repository: Some(to_proto_repo(repo)),
                        user: Some(to_proto_user(user)),
                        branch_name: branch_name.as_bytes().to_vec(),
                        start_branch_name: start_branch_name
                            .unwrap_or_default()
                            .as_bytes()
                            .to_vec(),
                        commit_message: message.as_bytes().to_vec(),
                        commit_author_name: user.name.as_bytes().to_vec(),
                        commit_author_email: user.email.as_bytes().to_vec(),
                        expected_old_oid: expected_old_oid.unwrap_or_default().to_string(),
                        ..Default::default()
                    },
                )),
            });

            for action in actions.clone() {
                let (header, content) = match action {
                    FileAction::Create { path, content } => (
                        UserCommitFilesActionHeader {
                            action: ActionType::Create as i32,
                            file_path: path.as_bytes().to_vec(),
                            ..Default::default()
                        },
                        Some(content),
                    ),
                    FileAction::CreateDir { path } => (
                        UserCommitFilesActionHeader {
                            action: ActionType::CreateDir as i32,
                            file_path: path.as_bytes().to_vec(),
                            ..Default::default()
                        },
                        None,
                    ),
                    FileAction::Update { path, content } => (
                        UserCommitFilesActionHeader {
                            action: ActionType::Update as i32,
                            file_path: path.as_bytes().to_vec(),
                            ..Default::default()
                        },
                        Some(content),
                    ),
                    FileAction::Move {
                        previous_path,
                        path,
                        content,
                    } => {
                        let infer_content = content.is_none();
                        (
                            UserCommitFilesActionHeader {
                                action: ActionType::Move as i32,
                                file_path: path.as_bytes().to_vec(),
                                previous_path: previous_path.as_bytes().to_vec(),
                                infer_content,
                                ..Default::default()
                            },
                            content,
                        )
                    }
                    FileAction::Delete { path } => (
                        UserCommitFilesActionHeader {
                            action: ActionType::Delete as i32,
                            file_path: path.as_bytes().to_vec(),
                            ..Default::default()
                        },
                        None,
                    ),
                    FileAction::Chmod { path, execute } => (
                        UserCommitFilesActionHeader {
                            action: ActionType::Chmod as i32,
                            file_path: path.as_bytes().to_vec(),
                            execute_filemode: execute,
                            ..Default::default()
                        },
                        None,
                    ),
                };

                requests.push(UserCommitFilesRequest {
                    user_commit_files_request_payload: Some(UserCommitFilesRequestPayload::Action(
                        UserCommitFilesAction {
                            user_commit_files_action_payload: Some(
                                UserCommitFilesActionPayload::Header(header),
                            ),
                        },
                    )),
                });

                if let Some(content) = content {
                    requests.push(UserCommitFilesRequest {
                        user_commit_files_request_payload: Some(
                            UserCommitFilesRequestPayload::Action(UserCommitFilesAction {
                                user_commit_files_action_payload: Some(
                                    UserCommitFilesActionPayload::Content(content),
                                ),
                            }),
                        ),
                    });
                }
            }

            requests
        };

        let requests = build_requests(None);
        let retry_requests = requests.clone();

        let response = match client.user_commit_files(iter(requests)).await {
            Ok(response) => response,
            Err(status)
                if start_branch_name.is_none()
                    && status.code() == Code::InvalidArgument
                    && status
                        .message()
                        .contains("could not resolve parent commit from start_sha, start_branch_name, or branch_name") =>
            {
                client
                    .user_commit_files(iter(build_requests(Some(ZERO_OID))))
                    .await
                    .map_err(map_status_to_error)?
            }
            Err(status) => return Err(map_status_to_error(status)),
        };

        let response = response.into_inner();
        if let Some(branch_update) = response.branch_update {
            let commit_id = if branch_update.commit_id.is_empty() {
                self.find_commit(repo, branch_name)
                    .await?
                    .map(|c| c.id)
                    .unwrap_or_default()
            } else {
                branch_update.commit_id
            };
            if !commit_id.is_empty() {
                return Ok(CommitResult {
                    commit_id,
                    branch_created: branch_update.branch_created,
                });
            }
        }

        if let Some(commit) = self.find_commit(repo, branch_name).await? {
            return Ok(CommitResult {
                commit_id: commit.id,
                branch_created: false,
            });
        }

        if let Some(start_branch_name) = start_branch_name {
            let _ = self
                .create_branch(repo, user, branch_name, start_branch_name)
                .await;
            let retry = client
                .user_commit_files(iter(retry_requests))
                .await
                .map_err(map_status_to_error)?;
            if let Some(branch_update) = retry.into_inner().branch_update {
                let commit_id = if branch_update.commit_id.is_empty() {
                    self.find_commit(repo, branch_name)
                        .await?
                        .map(|c| c.id)
                        .unwrap_or_default()
                } else {
                    branch_update.commit_id
                };
                if !commit_id.is_empty() {
                    return Ok(CommitResult {
                        commit_id,
                        branch_created: branch_update.branch_created,
                    });
                }
            }
            if let Some(commit) = self.find_commit(repo, branch_name).await? {
                return Ok(CommitResult {
                    commit_id: commit.id,
                    branch_created: false,
                });
            }
        }

        Err(ConmanError::Git {
            message: "commit_files returned empty branch update".to_string(),
        })
    }

    async fn commit_diff(
        &self,
        repo: &GitRepo,
        left_sha: &str,
        right_sha: &str,
    ) -> Result<Vec<GitDiffEntry>, ConmanError> {
        let mut client = self.diff_client();
        let request = CommitDiffRequest {
            repository: Some(to_proto_repo(repo)),
            left_commit_id: left_sha.to_string(),
            right_commit_id: right_sha.to_string(),
            diff_mode: DiffMode::Default as i32,
            max_patch_bytes: i32::MAX,
            ..Default::default()
        };

        let response = client
            .commit_diff(request)
            .await
            .map_err(map_status_to_error)?;

        let mut stream = response.into_inner();
        let mut result = Vec::new();
        let mut current: Option<GitDiffEntry> = None;
        let mut current_key: Option<(String, String, String, String, i32, i32, bool)> = None;

        while let Some(chunk) = stream.message().await.map_err(map_status_to_error)? {
            let key = (
                bytes_to_string(chunk.from_path.clone()),
                bytes_to_string(chunk.to_path.clone()),
                chunk.from_id.clone(),
                chunk.to_id.clone(),
                chunk.old_mode,
                chunk.new_mode,
                chunk.binary,
            );

            if current_key.as_ref() != Some(&key) {
                if let Some(entry) = current.take() {
                    result.push(entry);
                }

                current_key = Some(key.clone());
                current = Some(GitDiffEntry {
                    from_path: key.0,
                    to_path: key.1,
                    from_id: key.2,
                    to_id: key.3,
                    old_mode: key.4,
                    new_mode: key.5,
                    binary: key.6,
                    patch: chunk.raw_patch_data,
                    lines_added: chunk.lines_added,
                    lines_removed: chunk.lines_removed,
                });
            } else if let Some(entry) = current.as_mut() {
                entry.patch.extend(chunk.raw_patch_data);
                entry.lines_added = chunk.lines_added;
                entry.lines_removed = chunk.lines_removed;
            }

            if chunk.end_of_patch {
                if let Some(entry) = current.take() {
                    result.push(entry);
                }
                current_key = None;
            }
        }

        if let Some(entry) = current {
            result.push(entry);
        }

        Ok(result)
    }

    async fn raw_diff(
        &self,
        repo: &GitRepo,
        left_sha: &str,
        right_sha: &str,
    ) -> Result<Vec<u8>, ConmanError> {
        let mut client = self.diff_client();
        let request = RawDiffRequest {
            repository: Some(to_proto_repo(repo)),
            left_commit_id: left_sha.to_string(),
            right_commit_id: right_sha.to_string(),
        };

        let response = client
            .raw_diff(request)
            .await
            .map_err(map_status_to_error)?;
        let mut stream = response.into_inner();
        let mut bytes = Vec::new();

        while let Some(chunk) = stream.message().await.map_err(map_status_to_error)? {
            bytes.extend(chunk.data);
        }

        Ok(bytes)
    }

    async fn diff_stats(
        &self,
        repo: &GitRepo,
        left_sha: &str,
        right_sha: &str,
    ) -> Result<Vec<GitDiffStat>, ConmanError> {
        let mut client = self.diff_client();
        let request = DiffStatsRequest {
            repository: Some(to_proto_repo(repo)),
            left_commit_id: left_sha.to_string(),
            right_commit_id: right_sha.to_string(),
        };

        let response = client
            .diff_stats(request)
            .await
            .map_err(map_status_to_error)?;

        let mut stream = response.into_inner();
        let mut stats = Vec::new();

        while let Some(response) = stream.message().await.map_err(map_status_to_error)? {
            stats.extend(response.stats.into_iter().map(|entry| GitDiffStat {
                path: bytes_to_string(entry.path),
                old_path: if entry.old_path.is_empty() {
                    None
                } else {
                    Some(bytes_to_string(entry.old_path))
                },
                additions: entry.additions,
                deletions: entry.deletions,
            }));
        }

        Ok(stats)
    }

    async fn find_commit(
        &self,
        repo: &GitRepo,
        revision: &str,
    ) -> Result<Option<GitCommit>, ConmanError> {
        let mut client = self.commit_client();
        let request = FindCommitRequest {
            repository: Some(to_proto_repo(repo)),
            revision: revision.as_bytes().to_vec(),
            trailers: false,
        };

        match client.find_commit(request).await {
            Ok(response) => Ok(response.into_inner().commit.map(commit_from_proto)),
            Err(status) if status_is_not_found(&status) => Ok(None),
            Err(status) => Err(map_status_to_error(status)),
        }
    }

    async fn list_commits(
        &self,
        repo: &GitRepo,
        revisions: Vec<String>,
        pagination: Option<(String, i32)>,
    ) -> Result<Vec<GitCommit>, ConmanError> {
        let mut client = self.commit_client();
        let request = ListCommitsRequest {
            repository: Some(to_proto_repo(repo)),
            revisions,
            pagination_params: pagination.map(|(page_token, limit)| {
                gitaly_proto::gitaly::PaginationParameter { page_token, limit }
            }),
            order: Order::None as i32,
            ..Default::default()
        };

        let response = client
            .list_commits(request)
            .await
            .map_err(map_status_to_error)?;

        let mut stream = response.into_inner();
        let mut commits = Vec::new();

        while let Some(response) = stream.message().await.map_err(map_status_to_error)? {
            commits.extend(response.commits.into_iter().map(commit_from_proto));
        }

        Ok(commits)
    }

    async fn is_ancestor(
        &self,
        repo: &GitRepo,
        ancestor_id: &str,
        child_id: &str,
    ) -> Result<bool, ConmanError> {
        let mut client = self.commit_client();
        let request = CommitIsAncestorRequest {
            repository: Some(to_proto_repo(repo)),
            ancestor_id: ancestor_id.to_string(),
            child_id: child_id.to_string(),
        };

        let response = client
            .commit_is_ancestor(request)
            .await
            .map_err(map_status_to_error)?;

        Ok(response.into_inner().value)
    }

    async fn merge_to_ref(
        &self,
        repo: &GitRepo,
        user: &GitUser,
        source_sha: &str,
        target_ref: &str,
        first_parent_ref: &str,
        message: &str,
    ) -> Result<String, ConmanError> {
        let mut client = self.operation_client();
        let request = UserMergeToRefRequest {
            repository: Some(to_proto_repo(repo)),
            user: Some(to_proto_user(user)),
            source_sha: source_sha.to_string(),
            target_ref: target_ref.as_bytes().to_vec(),
            first_parent_ref: first_parent_ref.as_bytes().to_vec(),
            message: message.as_bytes().to_vec(),
            ..Default::default()
        };

        let response = client
            .user_merge_to_ref(request)
            .await
            .map_err(map_status_to_error)?;

        Ok(response.into_inner().commit_id)
    }

    async fn merge_branch(
        &self,
        repo: &GitRepo,
        user: &GitUser,
        commit_id: &str,
        branch: &str,
        message: &str,
    ) -> Result<MergeResult, ConmanError> {
        let mut client = self.operation_client();
        let requests = vec![
            UserMergeBranchRequest {
                repository: Some(to_proto_repo(repo)),
                user: Some(to_proto_user(user)),
                commit_id: commit_id.to_string(),
                branch: branch.as_bytes().to_vec(),
                message: message.as_bytes().to_vec(),
                apply: false,
                ..Default::default()
            },
            UserMergeBranchRequest {
                apply: true,
                ..Default::default()
            },
        ];

        let response = client
            .user_merge_branch(iter(requests))
            .await
            .map_err(map_status_to_error)?;

        let mut stream = response.into_inner();
        let mut final_commit_id = String::new();

        while let Some(response) = stream.message().await.map_err(map_status_to_error)? {
            if let Some(commit_id) = commit_id_from_branch_update(response.branch_update) {
                final_commit_id = commit_id;
            }
            if final_commit_id.is_empty() && !response.commit_id.is_empty() {
                final_commit_id = response.commit_id;
            }
        }

        if final_commit_id.is_empty() {
            return Err(ConmanError::Git {
                message: "merge_branch returned no commit id".to_string(),
            });
        }

        Ok(MergeResult {
            commit_id: final_commit_id,
        })
    }

    async fn rebase_to_ref(
        &self,
        repo: &GitRepo,
        user: &GitUser,
        source_sha: &str,
        target_ref: &str,
        first_parent_ref: &str,
    ) -> Result<String, ConmanError> {
        let mut client = self.operation_client();
        let request = UserRebaseToRefRequest {
            repository: Some(to_proto_repo(repo)),
            user: Some(to_proto_user(user)),
            source_sha: source_sha.to_string(),
            target_ref: target_ref.as_bytes().to_vec(),
            first_parent_ref: first_parent_ref.as_bytes().to_vec(),
            ..Default::default()
        };

        let response = client
            .user_rebase_to_ref(request)
            .await
            .map_err(map_status_to_error)?;

        Ok(response.into_inner().commit_id)
    }

    async fn create_tag(
        &self,
        repo: &GitRepo,
        user: &GitUser,
        tag_name: &str,
        target_revision: &str,
        message: &str,
    ) -> Result<GitTag, ConmanError> {
        let mut client = self.operation_client();
        let request = UserCreateTagRequest {
            repository: Some(to_proto_repo(repo)),
            user: Some(to_proto_user(user)),
            tag_name: tag_name.as_bytes().to_vec(),
            target_revision: target_revision.as_bytes().to_vec(),
            message: message.as_bytes().to_vec(),
            ..Default::default()
        };

        let response = client
            .user_create_tag(request)
            .await
            .map_err(map_status_to_error)?;

        response
            .into_inner()
            .tag
            .map(tag_from_proto)
            .ok_or_else(|| ConmanError::Git {
                message: "create_tag returned empty tag".to_string(),
            })
    }

    async fn delete_tag(
        &self,
        repo: &GitRepo,
        user: &GitUser,
        tag_name: &str,
    ) -> Result<(), ConmanError> {
        let mut client = self.operation_client();
        let request = UserDeleteTagRequest {
            repository: Some(to_proto_repo(repo)),
            user: Some(to_proto_user(user)),
            tag_name: tag_name.as_bytes().to_vec(),
            ..Default::default()
        };

        client
            .user_delete_tag(request)
            .await
            .map_err(map_status_to_error)?;
        Ok(())
    }

    async fn find_tag(
        &self,
        repo: &GitRepo,
        tag_name: &str,
    ) -> Result<Option<GitTag>, ConmanError> {
        let mut client = self.ref_client();
        let request = FindTagRequest {
            repository: Some(to_proto_repo(repo)),
            tag_name: tag_name.as_bytes().to_vec(),
        };

        match client.find_tag(request).await {
            Ok(response) => Ok(response.into_inner().tag.map(tag_from_proto)),
            Err(status) if status_is_not_found(&status) => Ok(None),
            Err(status) => Err(map_status_to_error(status)),
        }
    }

    async fn list_tags(&self, repo: &GitRepo) -> Result<Vec<GitTag>, ConmanError> {
        let mut client = self.ref_client();
        let request = FindAllTagsRequest {
            repository: Some(to_proto_repo(repo)),
            ..Default::default()
        };

        let response = client
            .find_all_tags(request)
            .await
            .map_err(map_status_to_error)?;

        let mut stream = response.into_inner();
        let mut tags = Vec::new();

        while let Some(response) = stream.message().await.map_err(map_status_to_error)? {
            tags.extend(response.tags.into_iter().map(tag_from_proto));
        }

        Ok(tags)
    }

    async fn revert(
        &self,
        repo: &GitRepo,
        user: &GitUser,
        commit_id: &str,
        branch_name: &str,
        message: &str,
    ) -> Result<RevertResult, ConmanError> {
        let mut client = self.operation_client();
        let request = UserRevertRequest {
            repository: Some(to_proto_repo(repo)),
            user: Some(to_proto_user(user)),
            commit: Some(ProtoGitCommit {
                id: commit_id.to_string(),
                ..Default::default()
            }),
            branch_name: branch_name.as_bytes().to_vec(),
            message: message.as_bytes().to_vec(),
            ..Default::default()
        };

        let response = client
            .user_revert(request)
            .await
            .map_err(map_status_to_error)?;

        let response = response.into_inner();
        if !response.create_tree_error.is_empty() || !response.commit_error.is_empty() {
            let suffix = if response.commit_error.is_empty() {
                String::new()
            } else {
                format!(", {}", response.commit_error)
            };
            return Err(ConmanError::Git {
                message: format!("revert failed: {}{}", response.create_tree_error, suffix),
            });
        }

        let commit_id = response
            .branch_update
            .map(|update| update.commit_id)
            .unwrap_or_default();

        if commit_id.is_empty() {
            return Err(ConmanError::Git {
                message: "revert returned no commit id".to_string(),
            });
        }

        Ok(RevertResult { commit_id })
    }

    async fn update_references(
        &self,
        repo: &GitRepo,
        updates: Vec<RefUpdate>,
    ) -> Result<(), ConmanError> {
        let mut client = self.ref_client();
        let request = UpdateReferencesRequest {
            repository: Some(to_proto_repo(repo)),
            updates: updates
                .into_iter()
                .map(|update| update_references_request::Update {
                    reference: update.reference.as_bytes().to_vec(),
                    old_object_id: update.old_object_id.as_bytes().to_vec(),
                    new_object_id: update.new_object_id.as_bytes().to_vec(),
                })
                .collect(),
        };

        client
            .update_references(iter(vec![request]))
            .await
            .map_err(map_status_to_error)?;

        Ok(())
    }
}

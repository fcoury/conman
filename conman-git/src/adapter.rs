use async_trait::async_trait;
use conman_core::{
    CommitResult, ConmanError, FileAction, GitBranch, GitCommit, GitDiffEntry, GitDiffStat,
    GitRepo, GitTag, GitTreeEntry, GitUser, MergeResult, RefUpdate, RevertResult,
};

fn not_implemented(method: &str) -> ConmanError {
    ConmanError::Git {
        message: format!("git adapter method not implemented: {method}"),
    }
}

#[async_trait]
pub trait GitAdapter: Send + Sync + 'static {
    async fn create_repo(&self, _storage: &str, _path: &str) -> Result<(), ConmanError> {
        Err(not_implemented("create_repo"))
    }

    async fn repo_exists(&self, _repo: &GitRepo) -> Result<bool, ConmanError> {
        Err(not_implemented("repo_exists"))
    }

    async fn remove_repo(&self, _repo: &GitRepo) -> Result<(), ConmanError> {
        Err(not_implemented("remove_repo"))
    }

    async fn create_branch(
        &self,
        _repo: &GitRepo,
        _user: &GitUser,
        _branch_name: &str,
        _start_point: &str,
    ) -> Result<GitBranch, ConmanError> {
        Err(not_implemented("create_branch"))
    }

    async fn delete_branch(
        &self,
        _repo: &GitRepo,
        _user: &GitUser,
        _branch_name: &str,
    ) -> Result<(), ConmanError> {
        Err(not_implemented("delete_branch"))
    }

    async fn find_branch(
        &self,
        _repo: &GitRepo,
        _name: &str,
    ) -> Result<Option<GitBranch>, ConmanError> {
        Err(not_implemented("find_branch"))
    }

    async fn list_branches(&self, _repo: &GitRepo) -> Result<Vec<GitBranch>, ConmanError> {
        Err(not_implemented("list_branches"))
    }

    async fn get_tree_entries(
        &self,
        _repo: &GitRepo,
        _revision: &str,
        _path: &str,
        _recursive: bool,
    ) -> Result<Vec<GitTreeEntry>, ConmanError> {
        Err(not_implemented("get_tree_entries"))
    }

    async fn get_blob(
        &self,
        _repo: &GitRepo,
        _revision: &str,
        _path: &str,
    ) -> Result<Vec<u8>, ConmanError> {
        Err(not_implemented("get_blob"))
    }

    async fn commit_files(
        &self,
        _repo: &GitRepo,
        _user: &GitUser,
        _branch_name: &str,
        _message: &str,
        _actions: Vec<FileAction>,
    ) -> Result<CommitResult, ConmanError> {
        Err(not_implemented("commit_files"))
    }

    async fn commit_diff(
        &self,
        _repo: &GitRepo,
        _left_sha: &str,
        _right_sha: &str,
    ) -> Result<Vec<GitDiffEntry>, ConmanError> {
        Err(not_implemented("commit_diff"))
    }

    async fn raw_diff(
        &self,
        _repo: &GitRepo,
        _left_sha: &str,
        _right_sha: &str,
    ) -> Result<Vec<u8>, ConmanError> {
        Err(not_implemented("raw_diff"))
    }

    async fn diff_stats(
        &self,
        _repo: &GitRepo,
        _left_sha: &str,
        _right_sha: &str,
    ) -> Result<Vec<GitDiffStat>, ConmanError> {
        Err(not_implemented("diff_stats"))
    }

    async fn find_commit(
        &self,
        _repo: &GitRepo,
        _revision: &str,
    ) -> Result<Option<GitCommit>, ConmanError> {
        Err(not_implemented("find_commit"))
    }

    async fn list_commits(
        &self,
        _repo: &GitRepo,
        _revisions: Vec<String>,
        _pagination: Option<(String, i32)>,
    ) -> Result<Vec<GitCommit>, ConmanError> {
        Err(not_implemented("list_commits"))
    }

    async fn is_ancestor(
        &self,
        _repo: &GitRepo,
        _ancestor_id: &str,
        _child_id: &str,
    ) -> Result<bool, ConmanError> {
        Err(not_implemented("is_ancestor"))
    }

    async fn merge_to_ref(
        &self,
        _repo: &GitRepo,
        _user: &GitUser,
        _source_sha: &str,
        _target_ref: &str,
        _first_parent_ref: &str,
        _message: &str,
    ) -> Result<String, ConmanError> {
        Err(not_implemented("merge_to_ref"))
    }

    async fn merge_branch(
        &self,
        _repo: &GitRepo,
        _user: &GitUser,
        _commit_id: &str,
        _branch: &str,
        _message: &str,
    ) -> Result<MergeResult, ConmanError> {
        Err(not_implemented("merge_branch"))
    }

    async fn rebase_to_ref(
        &self,
        _repo: &GitRepo,
        _user: &GitUser,
        _source_sha: &str,
        _target_ref: &str,
        _first_parent_ref: &str,
    ) -> Result<String, ConmanError> {
        Err(not_implemented("rebase_to_ref"))
    }

    async fn create_tag(
        &self,
        _repo: &GitRepo,
        _user: &GitUser,
        _tag_name: &str,
        _target_revision: &str,
        _message: &str,
    ) -> Result<GitTag, ConmanError> {
        Err(not_implemented("create_tag"))
    }

    async fn delete_tag(
        &self,
        _repo: &GitRepo,
        _user: &GitUser,
        _tag_name: &str,
    ) -> Result<(), ConmanError> {
        Err(not_implemented("delete_tag"))
    }

    async fn find_tag(
        &self,
        _repo: &GitRepo,
        _tag_name: &str,
    ) -> Result<Option<GitTag>, ConmanError> {
        Err(not_implemented("find_tag"))
    }

    async fn list_tags(&self, _repo: &GitRepo) -> Result<Vec<GitTag>, ConmanError> {
        Err(not_implemented("list_tags"))
    }

    async fn revert(
        &self,
        _repo: &GitRepo,
        _user: &GitUser,
        _commit_id: &str,
        _branch_name: &str,
        _message: &str,
    ) -> Result<RevertResult, ConmanError> {
        Err(not_implemented("revert"))
    }

    async fn update_references(
        &self,
        _repo: &GitRepo,
        _updates: Vec<RefUpdate>,
    ) -> Result<(), ConmanError> {
        Err(not_implemented("update_references"))
    }
}

#[derive(Debug, Default, Clone)]
pub struct NoopGitAdapter;

#[async_trait]
impl GitAdapter for NoopGitAdapter {}

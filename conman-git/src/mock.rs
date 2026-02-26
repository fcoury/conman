use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use chrono::Utc;
use conman_core::{
    CommitResult, ConmanError, FileAction, GitAuthor, GitBranch, GitCommit, GitDiffEntry,
    GitDiffStat, GitRepo, GitTag, GitTreeEntry, GitUser, MergeResult, RefUpdate, RevertResult,
};

use crate::adapter::GitAdapter;

#[derive(Debug, Clone)]
pub struct MockCall {
    pub method: String,
    pub args: Vec<String>,
}

#[derive(Default)]
struct MockState {
    repos: HashSet<String>,
    branches: HashMap<String, HashMap<String, GitBranch>>,
    commits: HashMap<String, GitCommit>,
    tags: HashMap<String, HashMap<String, GitTag>>,
    blobs: HashMap<String, Vec<u8>>,
    call_log: Vec<MockCall>,
    next_id: usize,
}

#[derive(Clone, Default)]
pub struct MockGitalyClient {
    inner: Arc<Mutex<MockState>>,
}

impl MockGitalyClient {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn calls(&self) -> Vec<MockCall> {
        self.inner.lock().expect("lock").call_log.clone()
    }

    pub fn with_repo(self, repo: &GitRepo) -> Self {
        let key = repo_key(repo);
        self.inner.lock().expect("lock").repos.insert(key);
        self
    }

    pub fn with_blob(self, repo: &GitRepo, revision: &str, path: &str, data: Vec<u8>) -> Self {
        let key = format!("{}:{revision}:{path}", repo_key(repo));
        self.inner.lock().expect("lock").blobs.insert(key, data);
        self
    }

    fn record(&self, method: &str, args: Vec<String>) {
        self.inner.lock().expect("lock").call_log.push(MockCall {
            method: method.to_string(),
            args,
        });
    }

    fn next_commit_id(state: &mut MockState) -> String {
        state.next_id += 1;
        format!("mock-commit-{}", state.next_id)
    }
}

fn repo_key(repo: &GitRepo) -> String {
    format!("{}:{}", repo.storage_name, repo.relative_path)
}

fn branch_commit(branch_name: &str, start_point: &str) -> GitCommit {
    let now = Utc::now();
    GitCommit {
        id: start_point.to_string(),
        subject: format!("branch {branch_name}"),
        body: String::new(),
        author: GitAuthor {
            name: "mock".to_string(),
            email: "mock@example.com".to_string(),
            date: now,
        },
        committer: GitAuthor {
            name: "mock".to_string(),
            email: "mock@example.com".to_string(),
            date: now,
        },
        parent_ids: Vec::new(),
        tree_id: "mock-tree".to_string(),
    }
}

#[async_trait]
impl GitAdapter for MockGitalyClient {
    async fn create_repo(&self, storage: &str, path: &str) -> Result<(), ConmanError> {
        self.record("create_repo", vec![storage.to_string(), path.to_string()]);
        self.inner
            .lock()
            .expect("lock")
            .repos
            .insert(format!("{storage}:{path}"));
        Ok(())
    }

    async fn repo_exists(&self, repo: &GitRepo) -> Result<bool, ConmanError> {
        self.record("repo_exists", vec![repo_key(repo)]);
        Ok(self
            .inner
            .lock()
            .expect("lock")
            .repos
            .contains(&repo_key(repo)))
    }

    async fn remove_repo(&self, repo: &GitRepo) -> Result<(), ConmanError> {
        self.record("remove_repo", vec![repo_key(repo)]);
        let key = repo_key(repo);
        let mut st = self.inner.lock().expect("lock");
        st.repos.remove(&key);
        st.branches.remove(&key);
        Ok(())
    }

    async fn create_branch(
        &self,
        repo: &GitRepo,
        _user: &GitUser,
        branch_name: &str,
        start_point: &str,
    ) -> Result<GitBranch, ConmanError> {
        self.record(
            "create_branch",
            vec![
                repo_key(repo),
                branch_name.to_string(),
                start_point.to_string(),
            ],
        );

        let key = repo_key(repo);
        let branch = GitBranch {
            name: branch_name.to_string(),
            commit: branch_commit(branch_name, start_point),
        };
        let mut st = self.inner.lock().expect("lock");
        st.branches
            .entry(key)
            .or_default()
            .insert(branch_name.to_string(), branch.clone());
        Ok(branch)
    }

    async fn delete_branch(
        &self,
        repo: &GitRepo,
        _user: &GitUser,
        branch_name: &str,
    ) -> Result<(), ConmanError> {
        self.record(
            "delete_branch",
            vec![repo_key(repo), branch_name.to_string()],
        );
        if let Some(map) = self
            .inner
            .lock()
            .expect("lock")
            .branches
            .get_mut(&repo_key(repo))
        {
            map.remove(branch_name);
        }
        Ok(())
    }

    async fn find_branch(
        &self,
        repo: &GitRepo,
        name: &str,
    ) -> Result<Option<GitBranch>, ConmanError> {
        self.record("find_branch", vec![repo_key(repo), name.to_string()]);
        Ok(self
            .inner
            .lock()
            .expect("lock")
            .branches
            .get(&repo_key(repo))
            .and_then(|m| m.get(name))
            .cloned())
    }

    async fn list_branches(&self, repo: &GitRepo) -> Result<Vec<GitBranch>, ConmanError> {
        self.record("list_branches", vec![repo_key(repo)]);
        Ok(self
            .inner
            .lock()
            .expect("lock")
            .branches
            .get(&repo_key(repo))
            .map(|m| m.values().cloned().collect())
            .unwrap_or_default())
    }

    async fn get_tree_entries(
        &self,
        repo: &GitRepo,
        revision: &str,
        path: &str,
        recursive: bool,
    ) -> Result<Vec<GitTreeEntry>, ConmanError> {
        self.record(
            "get_tree_entries",
            vec![
                repo_key(repo),
                revision.to_string(),
                path.to_string(),
                recursive.to_string(),
            ],
        );
        Ok(Vec::new())
    }

    async fn get_blob(
        &self,
        repo: &GitRepo,
        revision: &str,
        path: &str,
    ) -> Result<Vec<u8>, ConmanError> {
        self.record(
            "get_blob",
            vec![repo_key(repo), revision.to_string(), path.to_string()],
        );
        let key = format!("{}:{revision}:{path}", repo_key(repo));
        self.inner
            .lock()
            .expect("lock")
            .blobs
            .get(&key)
            .cloned()
            .ok_or_else(|| ConmanError::NotFound {
                entity: "blob",
                id: key,
            })
    }

    async fn commit_files(
        &self,
        repo: &GitRepo,
        _user: &GitUser,
        branch_name: &str,
        _message: &str,
        _actions: Vec<FileAction>,
    ) -> Result<CommitResult, ConmanError> {
        self.record(
            "commit_files",
            vec![repo_key(repo), branch_name.to_string()],
        );

        let mut st = self.inner.lock().expect("lock");
        let commit_id = Self::next_commit_id(&mut st);
        st.commits
            .insert(commit_id.clone(), GitCommit::placeholder(commit_id.clone()));

        let repo_key = repo_key(repo);
        let branches = st.branches.entry(repo_key).or_default();
        let existed = branches.contains_key(branch_name);
        let branch = branches
            .entry(branch_name.to_string())
            .or_insert_with(|| GitBranch {
                name: branch_name.to_string(),
                commit: GitCommit::placeholder(commit_id.clone()),
            });
        branch.commit = GitCommit::placeholder(commit_id.clone());

        Ok(CommitResult {
            commit_id,
            branch_created: !existed,
        })
    }

    async fn commit_diff(
        &self,
        repo: &GitRepo,
        left_sha: &str,
        right_sha: &str,
    ) -> Result<Vec<GitDiffEntry>, ConmanError> {
        self.record(
            "commit_diff",
            vec![repo_key(repo), left_sha.to_string(), right_sha.to_string()],
        );
        Ok(Vec::new())
    }

    async fn raw_diff(
        &self,
        repo: &GitRepo,
        left_sha: &str,
        right_sha: &str,
    ) -> Result<Vec<u8>, ConmanError> {
        self.record(
            "raw_diff",
            vec![repo_key(repo), left_sha.to_string(), right_sha.to_string()],
        );
        Ok(Vec::new())
    }

    async fn diff_stats(
        &self,
        repo: &GitRepo,
        left_sha: &str,
        right_sha: &str,
    ) -> Result<Vec<GitDiffStat>, ConmanError> {
        self.record(
            "diff_stats",
            vec![repo_key(repo), left_sha.to_string(), right_sha.to_string()],
        );
        Ok(Vec::new())
    }

    async fn find_commit(
        &self,
        repo: &GitRepo,
        revision: &str,
    ) -> Result<Option<GitCommit>, ConmanError> {
        self.record("find_commit", vec![repo_key(repo), revision.to_string()]);
        Ok(self
            .inner
            .lock()
            .expect("lock")
            .commits
            .get(revision)
            .cloned())
    }

    async fn list_commits(
        &self,
        repo: &GitRepo,
        revisions: Vec<String>,
        _pagination: Option<(String, i32)>,
    ) -> Result<Vec<GitCommit>, ConmanError> {
        self.record("list_commits", vec![repo_key(repo)]);
        let st = self.inner.lock().expect("lock");
        let mut commits = Vec::new();
        for rev in revisions {
            if let Some(c) = st.commits.get(&rev) {
                commits.push(c.clone());
            }
        }
        Ok(commits)
    }

    async fn is_ancestor(
        &self,
        repo: &GitRepo,
        ancestor_id: &str,
        child_id: &str,
    ) -> Result<bool, ConmanError> {
        self.record(
            "is_ancestor",
            vec![
                repo_key(repo),
                ancestor_id.to_string(),
                child_id.to_string(),
            ],
        );
        if ancestor_id == child_id {
            return Ok(true);
        }
        let st = self.inner.lock().expect("lock");
        let result = st
            .commits
            .get(child_id)
            .map(|c| c.parent_ids.iter().any(|p| p == ancestor_id))
            .unwrap_or(false);
        Ok(result)
    }

    async fn merge_to_ref(
        &self,
        repo: &GitRepo,
        _user: &GitUser,
        source_sha: &str,
        target_ref: &str,
        first_parent_ref: &str,
        _message: &str,
    ) -> Result<String, ConmanError> {
        self.record(
            "merge_to_ref",
            vec![
                repo_key(repo),
                source_sha.to_string(),
                target_ref.to_string(),
                first_parent_ref.to_string(),
            ],
        );
        Ok(format!("merge-{source_sha}"))
    }

    async fn merge_branch(
        &self,
        repo: &GitRepo,
        _user: &GitUser,
        commit_id: &str,
        branch: &str,
        _message: &str,
    ) -> Result<MergeResult, ConmanError> {
        self.record(
            "merge_branch",
            vec![repo_key(repo), commit_id.to_string(), branch.to_string()],
        );
        Ok(MergeResult {
            commit_id: format!("merge-{commit_id}"),
        })
    }

    async fn rebase_to_ref(
        &self,
        repo: &GitRepo,
        _user: &GitUser,
        source_sha: &str,
        target_ref: &str,
        first_parent_ref: &str,
    ) -> Result<String, ConmanError> {
        self.record(
            "rebase_to_ref",
            vec![
                repo_key(repo),
                source_sha.to_string(),
                target_ref.to_string(),
                first_parent_ref.to_string(),
            ],
        );
        Ok(format!("rebase-{source_sha}"))
    }

    async fn create_tag(
        &self,
        repo: &GitRepo,
        _user: &GitUser,
        tag_name: &str,
        target_revision: &str,
        message: &str,
    ) -> Result<GitTag, ConmanError> {
        self.record(
            "create_tag",
            vec![
                repo_key(repo),
                tag_name.to_string(),
                target_revision.to_string(),
            ],
        );
        let tag = GitTag {
            name: tag_name.to_string(),
            id: format!("tag-{tag_name}"),
            target_commit: self
                .inner
                .lock()
                .expect("lock")
                .commits
                .get(target_revision)
                .cloned(),
            message: if message.is_empty() {
                None
            } else {
                Some(message.to_string())
            },
            tagger: None,
        };
        self.inner
            .lock()
            .expect("lock")
            .tags
            .entry(repo_key(repo))
            .or_default()
            .insert(tag_name.to_string(), tag.clone());
        Ok(tag)
    }

    async fn delete_tag(
        &self,
        repo: &GitRepo,
        _user: &GitUser,
        tag_name: &str,
    ) -> Result<(), ConmanError> {
        self.record("delete_tag", vec![repo_key(repo), tag_name.to_string()]);
        if let Some(tags) = self
            .inner
            .lock()
            .expect("lock")
            .tags
            .get_mut(&repo_key(repo))
        {
            tags.remove(tag_name);
        }
        Ok(())
    }

    async fn find_tag(
        &self,
        repo: &GitRepo,
        tag_name: &str,
    ) -> Result<Option<GitTag>, ConmanError> {
        self.record("find_tag", vec![repo_key(repo), tag_name.to_string()]);
        Ok(self
            .inner
            .lock()
            .expect("lock")
            .tags
            .get(&repo_key(repo))
            .and_then(|m| m.get(tag_name))
            .cloned())
    }

    async fn list_tags(&self, repo: &GitRepo) -> Result<Vec<GitTag>, ConmanError> {
        self.record("list_tags", vec![repo_key(repo)]);
        Ok(self
            .inner
            .lock()
            .expect("lock")
            .tags
            .get(&repo_key(repo))
            .map(|m| m.values().cloned().collect())
            .unwrap_or_default())
    }

    async fn revert(
        &self,
        repo: &GitRepo,
        _user: &GitUser,
        commit_id: &str,
        branch_name: &str,
        _message: &str,
    ) -> Result<RevertResult, ConmanError> {
        self.record(
            "revert",
            vec![
                repo_key(repo),
                commit_id.to_string(),
                branch_name.to_string(),
            ],
        );
        Ok(RevertResult {
            commit_id: format!("revert-{commit_id}"),
        })
    }

    async fn update_references(
        &self,
        repo: &GitRepo,
        updates: Vec<RefUpdate>,
    ) -> Result<(), ConmanError> {
        self.record(
            "update_references",
            vec![repo_key(repo), updates.len().to_string()],
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use chrono::Utc;

    use super::*;

    fn repo() -> GitRepo {
        GitRepo {
            storage_name: "default".to_string(),
            relative_path: "conman/sample.git".to_string(),
            gl_repository: "project-1".to_string(),
        }
    }

    fn user() -> GitUser {
        GitUser {
            gl_id: "1".to_string(),
            name: "Test".to_string(),
            email: "test@example.com".to_string(),
            gl_username: "test".to_string(),
            timezone: "UTC".to_string(),
        }
    }

    #[tokio::test]
    async fn repo_lifecycle_and_branch_ops() {
        let repo = repo();
        let mock = MockGitalyClient::new();

        assert!(!mock.repo_exists(&repo).await.expect("exists"));
        mock.create_repo(&repo.storage_name, &repo.relative_path)
            .await
            .expect("create");
        assert!(mock.repo_exists(&repo).await.expect("exists"));

        let branch = mock
            .create_branch(&repo, &user(), "feature/a", "abc123")
            .await
            .expect("create branch");
        assert_eq!(branch.name, "feature/a");

        let found = mock.find_branch(&repo, "feature/a").await.expect("find");
        assert!(found.is_some());

        let branches = mock.list_branches(&repo).await.expect("list");
        assert_eq!(branches.len(), 1);

        mock.delete_branch(&repo, &user(), "feature/a")
            .await
            .expect("delete");
        assert!(
            mock.find_branch(&repo, "feature/a")
                .await
                .expect("find")
                .is_none()
        );
    }

    #[tokio::test]
    async fn commit_and_tag_flow() {
        let repo = repo();
        let mock = MockGitalyClient::new().with_repo(&repo);

        let result = mock
            .commit_files(&repo, &user(), "main", "msg", Vec::new())
            .await
            .expect("commit");
        assert!(!result.commit_id.is_empty());

        let commit = GitCommit {
            id: result.commit_id.clone(),
            subject: "s".to_string(),
            body: String::new(),
            author: GitAuthor {
                name: "n".to_string(),
                email: "e@x".to_string(),
                date: Utc::now(),
            },
            committer: GitAuthor {
                name: "n".to_string(),
                email: "e@x".to_string(),
                date: Utc::now(),
            },
            parent_ids: Vec::new(),
            tree_id: "t".to_string(),
        };
        mock.inner
            .lock()
            .expect("lock")
            .commits
            .insert(commit.id.clone(), commit);

        let tag = mock
            .create_tag(
                &repo,
                &user(),
                "r2026.01.01.1",
                &result.commit_id,
                "release",
            )
            .await
            .expect("tag");
        assert_eq!(tag.name, "r2026.01.01.1");
        assert!(
            mock.find_tag(&repo, "r2026.01.01.1")
                .await
                .expect("find tag")
                .is_some()
        );
    }
}

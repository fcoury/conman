use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct GitRepo {
    pub storage_name: String,
    pub relative_path: String,
    pub gl_repository: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitUser {
    pub gl_id: String,
    pub name: String,
    pub email: String,
    pub gl_username: String,
    pub timezone: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitBranch {
    pub name: String,
    pub commit: GitCommit,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitCommit {
    pub id: String,
    pub subject: String,
    pub body: String,
    pub author: GitAuthor,
    pub committer: GitAuthor,
    pub parent_ids: Vec<String>,
    pub tree_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitAuthor {
    pub name: String,
    pub email: String,
    pub date: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitTag {
    pub name: String,
    pub id: String,
    pub target_commit: Option<GitCommit>,
    pub message: Option<String>,
    pub tagger: Option<GitAuthor>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitTreeEntry {
    pub oid: String,
    pub path: String,
    pub entry_type: GitTreeEntryType,
    pub mode: i32,
    pub flat_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum GitTreeEntryType {
    Blob,
    Tree,
    Commit,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitDiffEntry {
    pub from_path: String,
    pub to_path: String,
    pub from_id: String,
    pub to_id: String,
    pub old_mode: i32,
    pub new_mode: i32,
    pub binary: bool,
    pub patch: Vec<u8>,
    pub lines_added: i32,
    pub lines_removed: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitDiffStat {
    pub path: String,
    pub old_path: Option<String>,
    pub additions: i32,
    pub deletions: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitResult {
    pub commit_id: String,
    pub branch_created: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MergeResult {
    pub commit_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RevertResult {
    pub commit_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileAction {
    Create {
        path: String,
        content: Vec<u8>,
    },
    CreateDir {
        path: String,
    },
    Update {
        path: String,
        content: Vec<u8>,
    },
    Move {
        previous_path: String,
        path: String,
        content: Option<Vec<u8>>,
    },
    Delete {
        path: String,
    },
    Chmod {
        path: String,
        execute: bool,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RefUpdate {
    pub reference: String,
    pub old_object_id: String,
    pub new_object_id: String,
}

impl GitCommit {
    pub fn placeholder(id: impl Into<String>) -> Self {
        let now = Utc::now();
        let id = id.into();
        Self {
            id,
            subject: String::new(),
            body: String::new(),
            author: GitAuthor {
                name: String::new(),
                email: String::new(),
                date: now,
            },
            committer: GitAuthor {
                name: String::new(),
                email: String::new(),
                date: now,
            },
            parent_ids: Vec::new(),
            tree_id: String::new(),
        }
    }
}

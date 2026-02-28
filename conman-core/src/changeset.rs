use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{ConmanError, Role};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangesetState {
    Draft,
    Submitted,
    InReview,
    Approved,
    ChangesRequested,
    Rejected,
    Queued,
    Released,
    Conflicted,
    NeedsRevalidation,
}

impl ChangesetState {
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Released | Self::Rejected)
    }

    pub fn is_open(self) -> bool {
        !self.is_terminal()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangesetAction {
    Submit,
    Resubmit,
    Approve,
    RequestChanges,
    Reject,
    Queue,
    Release,
    MarkConflicted,
    MarkNeedsRevalidation,
    MoveToDraft,
}

pub fn transition(
    current: ChangesetState,
    action: ChangesetAction,
) -> Result<ChangesetState, ConmanError> {
    use ChangesetAction as A;
    use ChangesetState as S;
    let next = match (current, action) {
        (S::Draft, A::Submit) => S::Submitted,
        (S::Submitted, A::Resubmit) => S::Submitted,
        (S::InReview, A::Resubmit) => S::Submitted,
        (S::ChangesRequested, A::Resubmit) => S::Submitted,
        (S::Submitted, A::Approve) => S::Approved,
        (S::InReview, A::Approve) => S::Approved,
        (S::ChangesRequested, A::Approve) => S::Approved,
        (S::Submitted, A::RequestChanges) => S::ChangesRequested,
        (S::InReview, A::RequestChanges) => S::ChangesRequested,
        (S::ChangesRequested, A::RequestChanges) => S::ChangesRequested,
        (S::Submitted, A::Reject) => S::Rejected,
        (S::InReview, A::Reject) => S::Rejected,
        (S::ChangesRequested, A::Reject) => S::Rejected,
        (S::Approved, A::Queue) => S::Queued,
        (S::Queued, A::Release) => S::Released,
        (S::Queued, A::MarkConflicted) => S::Conflicted,
        (S::Queued, A::MarkNeedsRevalidation) => S::NeedsRevalidation,
        (S::ChangesRequested, A::MoveToDraft) => S::Draft,
        (S::Conflicted, A::MoveToDraft) => S::Draft,
        (S::NeedsRevalidation, A::MoveToDraft) => S::Draft,
        (S::Rejected, A::MoveToDraft) => S::Draft,
        _ => {
            return Err(ConmanError::InvalidTransition {
                from: format!("{current:?}"),
                to: format!("{action:?}"),
            });
        }
    };
    Ok(next)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Approval {
    pub user_id: String,
    pub role: Role,
    pub approved_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Changeset {
    pub id: String,
    pub repo_id: String,
    pub workspace_id: String,
    pub title: String,
    pub description: Option<String>,
    pub state: ChangesetState,
    pub author_user_id: String,
    pub head_sha: String,
    pub submitted_head_sha: Option<String>,
    pub revision: u32,
    pub approvals: Vec<Approval>,
    pub queue_position: Option<i64>,
    pub queued_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChangesetCommentEdit {
    pub previous_body: String,
    pub edited_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChangesetComment {
    pub id: String,
    pub repo_id: String,
    pub changeset_id: String,
    pub author_user_id: String,
    pub body: String,
    pub edits: Vec<ChangesetCommentEdit>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use super::{ChangesetAction as A, ChangesetState as S, transition};

    #[test]
    fn transition_happy_path() {
        let s1 = transition(S::Draft, A::Submit).expect("submit");
        assert_eq!(s1, S::Submitted);
        let s2 = transition(s1, A::Approve).expect("approve");
        assert_eq!(s2, S::Approved);
        let s3 = transition(s2, A::Queue).expect("queue");
        assert_eq!(s3, S::Queued);
        let s4 = transition(s3, A::Release).expect("release");
        assert_eq!(s4, S::Released);
    }

    #[test]
    fn transition_rejects_invalid() {
        assert!(transition(S::Draft, A::Approve).is_err());
    }
}

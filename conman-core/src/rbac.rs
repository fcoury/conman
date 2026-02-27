use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Member = 0,
    Reviewer = 1,
    ConfigManager = 2,
    Admin = 3,
    Owner = 4,
}

impl Role {
    pub fn satisfies(&self, required: Role) -> bool {
        *self >= required
    }
}

impl std::fmt::Display for Role {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Role::Member => write!(f, "member"),
            Role::Reviewer => write!(f, "reviewer"),
            Role::ConfigManager => write!(f, "config_manager"),
            Role::Admin => write!(f, "admin"),
            Role::Owner => write!(f, "owner"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Capability {
    ReadApp,
    EditOwnWorkspace,
    EditOwnChangeset,
    SubmitChangeset,
    CommentInReview,
    ReviewChangeset,
    MoveToDraftAny,
    MoveToDraftOwn,
    AssembleRelease,
    PublishRelease,
    DeployRelease,
    ApproveSkipStage,
    InviteUsers,
    ManageApp,
}

impl Capability {
    pub fn min_role(&self) -> Role {
        match self {
            Capability::ReadApp
            | Capability::EditOwnWorkspace
            | Capability::EditOwnChangeset
            | Capability::SubmitChangeset
            | Capability::CommentInReview
            | Capability::MoveToDraftOwn => Role::Member,
            Capability::ReviewChangeset | Capability::ApproveSkipStage => Role::Reviewer,
            Capability::MoveToDraftAny
            | Capability::AssembleRelease
            | Capability::PublishRelease
            | Capability::DeployRelease => Role::ConfigManager,
            Capability::InviteUsers | Capability::ManageApp => Role::Admin,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn role_ordering_satisfies() {
        assert!(Role::Owner.satisfies(Role::Admin));
        assert!(Role::Admin.satisfies(Role::ConfigManager));
        assert!(Role::ConfigManager.satisfies(Role::Reviewer));
        assert!(Role::Reviewer.satisfies(Role::Member));
        assert!(!Role::Member.satisfies(Role::Reviewer));
    }

    #[test]
    fn capability_min_role_map() {
        assert_eq!(Capability::ReadApp.min_role(), Role::Member);
        assert_eq!(Capability::ReviewChangeset.min_role(), Role::Reviewer);
        assert_eq!(Capability::PublishRelease.min_role(), Role::ConfigManager);
        assert_eq!(Capability::InviteUsers.min_role(), Role::Admin);
    }
}

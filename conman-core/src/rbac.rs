use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    User = 0,
    Reviewer = 1,
    ConfigManager = 2,
    AppAdmin = 3,
}

impl Role {
    pub fn satisfies(&self, required: Role) -> bool {
        *self >= required
    }
}

impl std::fmt::Display for Role {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Role::User => write!(f, "user"),
            Role::Reviewer => write!(f, "reviewer"),
            Role::ConfigManager => write!(f, "config_manager"),
            Role::AppAdmin => write!(f, "app_admin"),
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
            | Capability::MoveToDraftOwn => Role::User,
            Capability::ReviewChangeset | Capability::ApproveSkipStage => Role::Reviewer,
            Capability::MoveToDraftAny
            | Capability::AssembleRelease
            | Capability::PublishRelease
            | Capability::DeployRelease => Role::ConfigManager,
            Capability::InviteUsers | Capability::ManageApp => Role::AppAdmin,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn role_ordering_satisfies() {
        assert!(Role::AppAdmin.satisfies(Role::ConfigManager));
        assert!(Role::ConfigManager.satisfies(Role::Reviewer));
        assert!(Role::Reviewer.satisfies(Role::User));
        assert!(!Role::User.satisfies(Role::Reviewer));
    }

    #[test]
    fn capability_min_role_map() {
        assert_eq!(Capability::ReadApp.min_role(), Role::User);
        assert_eq!(Capability::ReviewChangeset.min_role(), Role::Reviewer);
        assert_eq!(Capability::PublishRelease.min_role(), Role::ConfigManager);
        assert_eq!(Capability::InviteUsers.min_role(), Role::AppAdmin);
    }
}

use std::collections::HashMap;

use conman_core::{Capability, ConmanError, Role};

#[derive(Debug, Clone)]
pub struct AuthUser {
    pub user_id: String,
    pub email: String,
    pub roles: HashMap<String, Role>,
}

impl AuthUser {
    pub fn require_role(&self, app_id: &str, required: Role) -> Result<(), ConmanError> {
        match self.roles.get(app_id) {
            Some(role) if role.satisfies(required) => Ok(()),
            _ => Err(ConmanError::Forbidden {
                message: format!("requires role {required} on app {app_id}"),
            }),
        }
    }

    pub fn require_capability(
        &self,
        app_id: &str,
        capability: Capability,
    ) -> Result<(), ConmanError> {
        self.require_role(app_id, capability.min_role())
    }

    pub fn role_for(&self, app_id: &str) -> Option<Role> {
        self.roles.get(app_id).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn require_role_works() {
        let mut roles = HashMap::new();
        roles.insert("app-1".to_string(), Role::ConfigManager);
        let user = AuthUser {
            user_id: "u1".to_string(),
            email: "u@example.com".to_string(),
            roles,
        };

        assert!(user.require_role("app-1", Role::Reviewer).is_ok());
        assert!(user.require_role("app-1", Role::Admin).is_err());
    }
}

use conman_core::ConmanError;
use serde::Deserialize;

const MAX_LIMIT: u64 = 100;
const DEFAULT_LIMIT: u64 = 20;
const DEFAULT_PAGE: u64 = 1;

#[derive(Debug, Clone, Deserialize)]
pub struct Pagination {
    #[serde(default = "default_page")]
    pub page: u64,

    #[serde(default = "default_limit")]
    pub limit: u64,
}

fn default_page() -> u64 {
    DEFAULT_PAGE
}

fn default_limit() -> u64 {
    DEFAULT_LIMIT
}

impl Pagination {
    pub fn validate(mut self) -> Result<Self, ConmanError> {
        if self.page == 0 {
            return Err(ConmanError::Validation {
                message: "page must be >= 1".to_string(),
            });
        }

        self.limit = self.limit.clamp(1, MAX_LIMIT);
        Ok(self)
    }

    pub fn skip(&self) -> u64 {
        (self.page - 1) * self.limit
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pagination_defaults() {
        let p = Pagination { page: 1, limit: 20 }.validate().expect("valid");
        assert_eq!(p.page, 1);
        assert_eq!(p.limit, 20);
        assert_eq!(p.skip(), 0);
    }

    #[test]
    fn pagination_rejects_zero_page() {
        let p = Pagination { page: 0, limit: 20 };
        assert!(p.validate().is_err());
    }

    #[test]
    fn pagination_clamps_excessive_limit() {
        let p = Pagination {
            page: 1,
            limit: 500,
        }
        .validate()
        .expect("valid");
        assert_eq!(p.limit, 100);
    }

    #[test]
    fn pagination_clamps_zero_limit() {
        let p = Pagination { page: 1, limit: 0 }.validate().expect("valid");
        assert_eq!(p.limit, 1);
    }

    #[test]
    fn pagination_skip_calculation() {
        let p = Pagination { page: 3, limit: 25 }.validate().expect("valid");
        assert_eq!(p.skip(), 50);
    }
}

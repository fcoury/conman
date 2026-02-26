#[derive(Debug, thiserror::Error)]
pub enum ConmanError {
    #[error("not found: {entity} {id}")]
    NotFound { entity: &'static str, id: String },

    #[error("conflict: {message}")]
    Conflict { message: String },

    #[error("forbidden: {message}")]
    Forbidden { message: String },

    #[error("unauthorized: {message}")]
    Unauthorized { message: String },

    #[error("validation: {message}")]
    Validation { message: String },

    #[error("invalid state transition: {from} -> {to}")]
    InvalidTransition { from: String, to: String },

    #[error("git error: {message}")]
    Git { message: String },

    #[error("internal: {message}")]
    Internal { message: String },
}

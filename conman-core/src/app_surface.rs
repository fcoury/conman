use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SurfaceBranding {
    pub header_logo: Option<String>,
    pub app_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSurface {
    pub id: String,
    pub repo_id: String,
    pub key: String,
    pub title: String,
    pub domains: Vec<String>,
    pub branding: Option<SurfaceBranding>,
    pub roles: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}


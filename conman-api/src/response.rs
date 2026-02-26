use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct ApiResponse<T: Serialize> {
    pub data: T,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub pagination: Option<PaginationMeta>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PaginationMeta {
    pub page: u64,
    pub limit: u64,
    pub total: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ApiError {
    pub error: ApiErrorBody,
}

#[derive(Debug, Clone, Serialize)]
pub struct ApiErrorBody {
    pub code: &'static str,
    pub message: String,
    pub request_id: String,
}

impl<T: Serialize> ApiResponse<T> {
    pub fn ok(data: T) -> Self {
        Self {
            data,
            pagination: None,
        }
    }

    pub fn paginated(data: T, page: u64, limit: u64, total: u64) -> Self {
        Self {
            data,
            pagination: Some(PaginationMeta { page, limit, total }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_success_without_pagination() {
        let body = ApiResponse::ok(serde_json::json!({ "k": "v" }));
        let value = serde_json::to_value(body).expect("json");
        assert!(value.get("pagination").is_none());
        assert_eq!(value["data"]["k"], "v");
    }

    #[test]
    fn serializes_success_with_pagination() {
        let body = ApiResponse::paginated(serde_json::json!([1, 2]), 2, 20, 42);
        let value = serde_json::to_value(body).expect("json");
        assert_eq!(value["pagination"]["page"], 2);
        assert_eq!(value["pagination"]["limit"], 20);
        assert_eq!(value["pagination"]["total"], 42);
    }
}

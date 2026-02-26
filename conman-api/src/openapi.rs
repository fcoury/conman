use std::collections::BTreeMap;

use axum::Json;
use axum::response::{Html, IntoResponse};
use serde_json::{Map, Value, json};

#[derive(Debug, Clone, Copy)]
struct RouteDoc {
    method: &'static str,
    path: &'static str,
    summary: &'static str,
    tag: &'static str,
    protected: bool,
}

const ROUTES: &[RouteDoc] = &[
    RouteDoc {
        method: "get",
        path: "/api/health",
        summary: "Get service health",
        tag: "platform",
        protected: false,
    },
    RouteDoc {
        method: "get",
        path: "/api/metrics",
        summary: "Scrape Prometheus metrics",
        tag: "platform",
        protected: false,
    },
    RouteDoc {
        method: "post",
        path: "/api/auth/login",
        summary: "Authenticate with email/password",
        tag: "auth",
        protected: false,
    },
    RouteDoc {
        method: "post",
        path: "/api/auth/logout",
        summary: "Logout current session",
        tag: "auth",
        protected: true,
    },
    RouteDoc {
        method: "post",
        path: "/api/auth/forgot-password",
        summary: "Request password reset",
        tag: "auth",
        protected: false,
    },
    RouteDoc {
        method: "post",
        path: "/api/auth/reset-password",
        summary: "Reset password with token",
        tag: "auth",
        protected: false,
    },
    RouteDoc {
        method: "post",
        path: "/api/auth/accept-invite",
        summary: "Accept app invite",
        tag: "auth",
        protected: false,
    },
    RouteDoc {
        method: "get",
        path: "/api/tenants",
        summary: "List tenants",
        tag: "tenants",
        protected: true,
    },
    RouteDoc {
        method: "post",
        path: "/api/tenants",
        summary: "Create tenant",
        tag: "tenants",
        protected: true,
    },
    RouteDoc {
        method: "get",
        path: "/api/tenants/{tenantId}",
        summary: "Get tenant",
        tag: "tenants",
        protected: true,
    },
    RouteDoc {
        method: "post",
        path: "/api/tenants/{tenantId}/repos",
        summary: "Create repository under tenant",
        tag: "tenants",
        protected: true,
    },
    RouteDoc {
        method: "get",
        path: "/api/repos",
        summary: "List repositories",
        tag: "repos",
        protected: true,
    },
    RouteDoc {
        method: "get",
        path: "/api/repos/{appId}",
        summary: "Get repository",
        tag: "repos",
        protected: true,
    },
    RouteDoc {
        method: "get",
        path: "/api/repos/{appId}/surfaces",
        summary: "List app surfaces",
        tag: "repos",
        protected: true,
    },
    RouteDoc {
        method: "post",
        path: "/api/repos/{appId}/surfaces",
        summary: "Create app surface",
        tag: "repos",
        protected: true,
    },
    RouteDoc {
        method: "patch",
        path: "/api/repos/{appId}/surfaces/{surfaceId}",
        summary: "Update app surface",
        tag: "repos",
        protected: true,
    },
    RouteDoc {
        method: "get",
        path: "/api/apps",
        summary: "List apps",
        tag: "apps",
        protected: true,
    },
    RouteDoc {
        method: "post",
        path: "/api/apps",
        summary: "Create app",
        tag: "apps",
        protected: true,
    },
    RouteDoc {
        method: "get",
        path: "/api/apps/{appId}",
        summary: "Get app",
        tag: "apps",
        protected: true,
    },
    RouteDoc {
        method: "patch",
        path: "/api/apps/{appId}/settings",
        summary: "Update app settings",
        tag: "apps",
        protected: true,
    },
    RouteDoc {
        method: "get",
        path: "/api/apps/{appId}/members",
        summary: "List app members",
        tag: "apps",
        protected: true,
    },
    RouteDoc {
        method: "post",
        path: "/api/apps/{appId}/members",
        summary: "Assign/replace member role",
        tag: "apps",
        protected: true,
    },
    RouteDoc {
        method: "post",
        path: "/api/apps/{appId}/invites",
        summary: "Create invite",
        tag: "apps",
        protected: true,
    },
    RouteDoc {
        method: "get",
        path: "/api/apps/{appId}/workspaces",
        summary: "List workspaces",
        tag: "workspaces",
        protected: true,
    },
    RouteDoc {
        method: "post",
        path: "/api/apps/{appId}/workspaces",
        summary: "Create workspace",
        tag: "workspaces",
        protected: true,
    },
    RouteDoc {
        method: "get",
        path: "/api/apps/{appId}/workspaces/{workspaceId}",
        summary: "Get workspace",
        tag: "workspaces",
        protected: true,
    },
    RouteDoc {
        method: "patch",
        path: "/api/apps/{appId}/workspaces/{workspaceId}",
        summary: "Update workspace metadata",
        tag: "workspaces",
        protected: true,
    },
    RouteDoc {
        method: "post",
        path: "/api/apps/{appId}/workspaces/{workspaceId}/reset",
        summary: "Reset workspace to baseline",
        tag: "workspaces",
        protected: true,
    },
    RouteDoc {
        method: "post",
        path: "/api/apps/{appId}/workspaces/{workspaceId}/sync-integration",
        summary: "Sync workspace with integration branch",
        tag: "workspaces",
        protected: true,
    },
    RouteDoc {
        method: "get",
        path: "/api/apps/{appId}/workspaces/{workspaceId}/files",
        summary: "Get workspace file or tree",
        tag: "workspaces",
        protected: true,
    },
    RouteDoc {
        method: "put",
        path: "/api/apps/{appId}/workspaces/{workspaceId}/files",
        summary: "Write workspace file",
        tag: "workspaces",
        protected: true,
    },
    RouteDoc {
        method: "delete",
        path: "/api/apps/{appId}/workspaces/{workspaceId}/files",
        summary: "Delete workspace file",
        tag: "workspaces",
        protected: true,
    },
    RouteDoc {
        method: "post",
        path: "/api/apps/{appId}/workspaces/{workspaceId}/checkpoints",
        summary: "Create workspace checkpoint",
        tag: "workspaces",
        protected: true,
    },
    RouteDoc {
        method: "get",
        path: "/api/apps/{appId}/changesets",
        summary: "List changesets",
        tag: "changesets",
        protected: true,
    },
    RouteDoc {
        method: "post",
        path: "/api/apps/{appId}/changesets",
        summary: "Create changeset",
        tag: "changesets",
        protected: true,
    },
    RouteDoc {
        method: "get",
        path: "/api/apps/{appId}/changesets/{changesetId}",
        summary: "Get changeset detail",
        tag: "changesets",
        protected: true,
    },
    RouteDoc {
        method: "patch",
        path: "/api/apps/{appId}/changesets/{changesetId}",
        summary: "Update changeset",
        tag: "changesets",
        protected: true,
    },
    RouteDoc {
        method: "post",
        path: "/api/apps/{appId}/changesets/{changesetId}/submit",
        summary: "Submit changeset",
        tag: "changesets",
        protected: true,
    },
    RouteDoc {
        method: "post",
        path: "/api/apps/{appId}/changesets/{changesetId}/resubmit",
        summary: "Resubmit changeset",
        tag: "changesets",
        protected: true,
    },
    RouteDoc {
        method: "post",
        path: "/api/apps/{appId}/changesets/{changesetId}/review",
        summary: "Review changeset",
        tag: "changesets",
        protected: true,
    },
    RouteDoc {
        method: "post",
        path: "/api/apps/{appId}/changesets/{changesetId}/queue",
        summary: "Queue approved changeset",
        tag: "changesets",
        protected: true,
    },
    RouteDoc {
        method: "post",
        path: "/api/apps/{appId}/changesets/{changesetId}/move-to-draft",
        summary: "Move queued changeset back to draft",
        tag: "changesets",
        protected: true,
    },
    RouteDoc {
        method: "get",
        path: "/api/apps/{appId}/changesets/{changesetId}/diff",
        summary: "Get changeset diff",
        tag: "changesets",
        protected: true,
    },
    RouteDoc {
        method: "get",
        path: "/api/apps/{appId}/changesets/{changesetId}/comments",
        summary: "List changeset comments",
        tag: "changesets",
        protected: true,
    },
    RouteDoc {
        method: "post",
        path: "/api/apps/{appId}/changesets/{changesetId}/comments",
        summary: "Create changeset comment",
        tag: "changesets",
        protected: true,
    },
    RouteDoc {
        method: "get",
        path: "/api/apps/{appId}/releases",
        summary: "List releases",
        tag: "releases",
        protected: true,
    },
    RouteDoc {
        method: "post",
        path: "/api/apps/{appId}/releases",
        summary: "Create release draft",
        tag: "releases",
        protected: true,
    },
    RouteDoc {
        method: "get",
        path: "/api/apps/{appId}/releases/{releaseId}",
        summary: "Get release",
        tag: "releases",
        protected: true,
    },
    RouteDoc {
        method: "post",
        path: "/api/apps/{appId}/releases/{releaseId}/changesets",
        summary: "Set release changeset selection",
        tag: "releases",
        protected: true,
    },
    RouteDoc {
        method: "post",
        path: "/api/apps/{appId}/releases/{releaseId}/reorder",
        summary: "Reorder release changesets",
        tag: "releases",
        protected: true,
    },
    RouteDoc {
        method: "post",
        path: "/api/apps/{appId}/releases/{releaseId}/assemble",
        summary: "Assemble release candidate",
        tag: "releases",
        protected: true,
    },
    RouteDoc {
        method: "post",
        path: "/api/apps/{appId}/releases/{releaseId}/publish",
        summary: "Publish release",
        tag: "releases",
        protected: true,
    },
    RouteDoc {
        method: "get",
        path: "/api/apps/{appId}/environments",
        summary: "List environments",
        tag: "environments",
        protected: true,
    },
    RouteDoc {
        method: "patch",
        path: "/api/apps/{appId}/environments",
        summary: "Replace environments",
        tag: "environments",
        protected: true,
    },
    RouteDoc {
        method: "get",
        path: "/api/apps/{appId}/runtime-profiles",
        summary: "List runtime profiles",
        tag: "runtime-profiles",
        protected: true,
    },
    RouteDoc {
        method: "post",
        path: "/api/apps/{appId}/runtime-profiles",
        summary: "Create runtime profile",
        tag: "runtime-profiles",
        protected: true,
    },
    RouteDoc {
        method: "get",
        path: "/api/apps/{appId}/runtime-profiles/{profileId}",
        summary: "Get runtime profile",
        tag: "runtime-profiles",
        protected: true,
    },
    RouteDoc {
        method: "patch",
        path: "/api/apps/{appId}/runtime-profiles/{profileId}",
        summary: "Update runtime profile",
        tag: "runtime-profiles",
        protected: true,
    },
    RouteDoc {
        method: "post",
        path: "/api/apps/{appId}/runtime-profiles/{profileId}/secrets/{key}/reveal",
        summary: "Reveal runtime profile secret",
        tag: "runtime-profiles",
        protected: true,
    },
    RouteDoc {
        method: "post",
        path: "/api/apps/{appId}/environments/{envId}/deploy",
        summary: "Deploy environment",
        tag: "deployments",
        protected: true,
    },
    RouteDoc {
        method: "post",
        path: "/api/apps/{appId}/environments/{envId}/promote",
        summary: "Promote environment",
        tag: "deployments",
        protected: true,
    },
    RouteDoc {
        method: "post",
        path: "/api/apps/{appId}/environments/{envId}/rollback",
        summary: "Rollback environment",
        tag: "deployments",
        protected: true,
    },
    RouteDoc {
        method: "get",
        path: "/api/apps/{appId}/deployments",
        summary: "List deployments",
        tag: "deployments",
        protected: true,
    },
    RouteDoc {
        method: "get",
        path: "/api/apps/{appId}/temp-envs",
        summary: "List temporary environments",
        tag: "temp-envs",
        protected: true,
    },
    RouteDoc {
        method: "post",
        path: "/api/apps/{appId}/temp-envs",
        summary: "Create temporary environment",
        tag: "temp-envs",
        protected: true,
    },
    RouteDoc {
        method: "post",
        path: "/api/apps/{appId}/temp-envs/{tempEnvId}/extend",
        summary: "Extend temporary environment TTL",
        tag: "temp-envs",
        protected: true,
    },
    RouteDoc {
        method: "post",
        path: "/api/apps/{appId}/temp-envs/{tempEnvId}/undo-expire",
        summary: "Undo temporary environment expiry",
        tag: "temp-envs",
        protected: true,
    },
    RouteDoc {
        method: "delete",
        path: "/api/apps/{appId}/temp-envs/{tempEnvId}",
        summary: "Delete temporary environment",
        tag: "temp-envs",
        protected: true,
    },
    RouteDoc {
        method: "get",
        path: "/api/apps/{appId}/jobs",
        summary: "List async jobs",
        tag: "jobs",
        protected: true,
    },
    RouteDoc {
        method: "get",
        path: "/api/apps/{appId}/jobs/{jobId}",
        summary: "Get async job detail",
        tag: "jobs",
        protected: true,
    },
    RouteDoc {
        method: "get",
        path: "/api/me/notification-preferences",
        summary: "Get notification preferences",
        tag: "me",
        protected: true,
    },
    RouteDoc {
        method: "patch",
        path: "/api/me/notification-preferences",
        summary: "Update notification preferences",
        tag: "me",
        protected: true,
    },
];

fn operation(route: &RouteDoc) -> Value {
    let mut op = Map::new();
    op.insert("summary".to_string(), json!(route.summary));
    op.insert("operationId".to_string(), json!(operation_id(route)));
    op.insert("tags".to_string(), json!([route.tag]));
    op.insert(
        "responses".to_string(),
        json!({
            "200": {
                "description": "Success",
                "content": {
                    "application/json": {
                        "schema": { "$ref": "#/components/schemas/ApiResponse" }
                    }
                }
            },
            "400": { "$ref": "#/components/responses/BadRequest" },
            "401": { "$ref": "#/components/responses/Unauthorized" },
            "403": { "$ref": "#/components/responses/Forbidden" },
            "404": { "$ref": "#/components/responses/NotFound" },
            "409": { "$ref": "#/components/responses/Conflict" },
            "500": { "$ref": "#/components/responses/InternalError" }
        }),
    );
    if route.protected {
        op.insert("security".to_string(), json!([{ "bearerAuth": [] }]));
    }
    Value::Object(op)
}

fn operation_id(route: &RouteDoc) -> String {
    let normalized = route
        .path
        .trim_start_matches('/')
        .replace("/api/", "")
        .replace("/{", "/by_")
        .replace('}', "")
        .replace(['/', '-'], "_");
    format!("{}_{}", route.method, normalized)
}

fn build_paths() -> BTreeMap<&'static str, Map<String, Value>> {
    let mut paths: BTreeMap<&'static str, Map<String, Value>> = BTreeMap::new();
    for route in ROUTES {
        let entry = paths.entry(route.path).or_default();
        entry.insert(route.method.to_string(), operation(route));
    }
    paths
}

pub fn openapi_document() -> Value {
    let mut paths_obj = Map::new();
    for (path, methods) in build_paths() {
        paths_obj.insert(path.to_string(), Value::Object(methods));
    }

    json!({
        "openapi": "3.1.0",
        "info": {
            "title": "Conman API",
            "version": "v1-draft",
            "description": "Current implemented Conman API surface."
        },
        "servers": [
            { "url": "/" }
        ],
        "paths": paths_obj,
        "components": {
            "securitySchemes": {
                "bearerAuth": {
                    "type": "http",
                    "scheme": "bearer",
                    "bearerFormat": "JWT"
                }
            },
            "schemas": {
                "ApiResponse": {
                    "type": "object",
                    "properties": {
                        "data": {},
                        "pagination": { "$ref": "#/components/schemas/Pagination" }
                    },
                    "required": ["data"]
                },
                "ApiError": {
                    "type": "object",
                    "properties": {
                        "error": {
                            "type": "object",
                            "properties": {
                                "code": { "type": "string" },
                                "message": { "type": "string" },
                                "request_id": { "type": "string" }
                            },
                            "required": ["code", "message", "request_id"]
                        }
                    },
                    "required": ["error"]
                },
                "Pagination": {
                    "type": "object",
                    "properties": {
                        "page": { "type": "integer", "minimum": 1 },
                        "limit": { "type": "integer", "minimum": 1 },
                        "total": { "type": "integer", "minimum": 0 }
                    },
                    "required": ["page", "limit", "total"]
                }
            },
            "responses": {
                "BadRequest": {
                    "description": "Bad request",
                    "content": { "application/json": { "schema": { "$ref": "#/components/schemas/ApiError" } } }
                },
                "Unauthorized": {
                    "description": "Unauthorized",
                    "content": { "application/json": { "schema": { "$ref": "#/components/schemas/ApiError" } } }
                },
                "Forbidden": {
                    "description": "Forbidden",
                    "content": { "application/json": { "schema": { "$ref": "#/components/schemas/ApiError" } } }
                },
                "NotFound": {
                    "description": "Not found",
                    "content": { "application/json": { "schema": { "$ref": "#/components/schemas/ApiError" } } }
                },
                "Conflict": {
                    "description": "Conflict",
                    "content": { "application/json": { "schema": { "$ref": "#/components/schemas/ApiError" } } }
                },
                "InternalError": {
                    "description": "Internal error",
                    "content": { "application/json": { "schema": { "$ref": "#/components/schemas/ApiError" } } }
                }
            }
        }
    })
}

pub async fn openapi_json() -> impl IntoResponse {
    Json(openapi_document())
}

pub async fn openapi_docs() -> impl IntoResponse {
    Html(
        r#"<!doctype html>
<html lang="en">
  <head>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <title>Conman API Docs</title>
    <link rel="stylesheet" href="https://unpkg.com/swagger-ui-dist@5/swagger-ui.css" />
  </head>
  <body>
    <div id="swagger-ui"></div>
    <script src="https://unpkg.com/swagger-ui-dist@5/swagger-ui-bundle.js"></script>
    <script src="https://unpkg.com/swagger-ui-dist@5/swagger-ui-standalone-preset.js"></script>
    <script>
      window.ui = SwaggerUIBundle({
        url: '/api/openapi.json',
        dom_id: '#swagger-ui',
        presets: [
          SwaggerUIBundle.presets.apis,
          SwaggerUIStandalonePreset
        ],
        layout: "StandaloneLayout",
        deepLinking: true,
        persistAuthorization: true,
        docExpansion: 'none'
      });
    </script>
  </body>
</html>"#,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn openapi_includes_core_paths() {
        let doc = openapi_document();
        let paths = doc
            .get("paths")
            .and_then(Value::as_object)
            .expect("paths object");
        assert!(paths.contains_key("/api/auth/login"));
        assert!(paths.contains_key("/api/apps/{appId}/changesets/{changesetId}/submit"));
        assert!(paths.contains_key("/api/apps/{appId}/releases/{releaseId}/publish"));
    }

    #[test]
    fn protected_routes_have_security() {
        let doc = openapi_document();
        let submit = &doc["paths"]["/api/apps/{appId}/changesets/{changesetId}/submit"]["post"];
        assert!(submit.get("security").is_some());
        let login = &doc["paths"]["/api/auth/login"]["post"];
        assert!(login.get("security").is_none());
    }
}

# E11 Notifications & Audit Completeness

## 1. Goal

Deliver full observability of user-visible events through email notifications and
an immutable, append-only audit log. After this epic, every privileged or
critical action in the system is captured in the `audit_events` collection with
structured before/after snapshots, and users receive email notifications for all
scoped events when their notification preference is enabled.

**Issues:**

- E11-01: Email templates and provider integration.
- E11-02: Per-user on/off notification preferences.
- E11-03: Event fanout for required notifications.
- E11-04: Append-only audit event writer + schema enforcement.
- E11-05: Backfill audit for critical legacy transitions (if any).

---

## 2. Dependencies

| Dependency | What it provides |
|------------|------------------|
| E05 Changesets | `Changeset`, `ChangesetState`, changeset lifecycle handlers that emit audit and notification events |
| E07 Queue Orchestration | Queue transitions, revalidation loop events |
| E08 Releases | Release assembly, publish, tag lifecycle events |
| E09 Deployments | Deployment start, succeed, fail, promote, rollback events |
| E10 Temp Environments | Temp env create, extend, expire, undo events |

---

## 3. Rust Types

### 3.1 `conman-core/src/models/audit.rs`

```rust
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Immutable audit event capturing a single action in the system.
///
/// Audit events are append-only: they are inserted but never updated or deleted.
/// Each event records who did what, to which entity, with optional before/after
/// snapshots for reconstructing state changes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEvent {
    /// MongoDB ObjectId hex string. Assigned on insert.
    pub id: String,

    /// When the action occurred.
    pub occurred_at: DateTime<Utc>,

    /// The user who performed the action. None for system-initiated actions
    /// (e.g., TTL-based temp env expiration).
    pub actor_user_id: Option<String>,

    /// The app this event belongs to. None for cross-app actions (e.g., user
    /// notification preference changes).
    pub app_id: Option<String>,

    /// The type of entity affected (e.g., "workspace", "changeset", "release",
    /// "deployment", "temp_environment", "app", "membership", "invite",
    /// "environment", "comment", "notification_preference").
    pub entity_type: String,

    /// The id of the affected entity.
    pub entity_id: String,

    /// The action performed (e.g., "created", "submitted", "approved",
    /// "published", "deployed", "settings_updated"). See section 6.3 for the
    /// full enumeration.
    pub action: String,

    /// Snapshot of the entity state before the action, if applicable.
    pub before: Option<serde_json::Value>,

    /// Snapshot of the entity state after the action, if applicable.
    pub after: Option<serde_json::Value>,

    /// The Git SHA associated with this action, if relevant (e.g., changeset
    /// head_sha at submit, release published_sha).
    pub git_sha: Option<String>,

    /// Request context captured at the time of the action.
    pub context: RequestContext,
}

/// Request metadata captured alongside every audit event for traceability.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestContext {
    /// Client IP address, from connection or X-Forwarded-For.
    pub ip: Option<String>,

    /// User-Agent header value.
    pub user_agent: Option<String>,

    /// The request ID that triggered this action.
    pub request_id: String,
}
```

### 3.2 `conman-core/src/models/notification.rs`

```rust
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Per-user notification preferences. V1 supports a single on/off toggle
/// for email notifications.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationPreference {
    /// MongoDB ObjectId hex string.
    pub id: String,

    /// The user this preference belongs to. Unique — one preference doc per user.
    pub user_id: String,

    /// Whether the user receives email notifications. Default: true.
    pub email_enabled: bool,

    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl NotificationPreference {
    /// Create a default preference for a new user (email enabled).
    pub fn default_for_user(user_id: String) -> Self {
        let now = Utc::now();
        Self {
            id: String::new(), // Assigned on insert
            user_id,
            email_enabled: true,
            created_at: now,
            updated_at: now,
        }
    }
}

/// All notification-worthy events in the system.
///
/// Each variant maps to an email template and a set of recipient resolution
/// rules. See section 6.1 for the fanout logic.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NotificationEvent {
    /// A changeset was submitted for review.
    ChangesetSubmitted,
    /// A user was explicitly requested to review a changeset.
    ReviewRequested,
    /// A changeset was approved by a reviewer.
    ChangesetApproved,
    /// A reviewer requested changes on a changeset.
    ChangesRequested,
    /// A changeset was rejected by a reviewer.
    ChangesetRejected,
    /// An approved changeset was moved to the queue.
    ChangesetQueued,
    /// A new release draft was created.
    ReleaseCreated,
    /// A release was published (tagged and merged to the integration branch).
    ReleasePublished,
    /// A deployment to an environment has started.
    DeploymentStarted,
    /// A deployment to an environment completed successfully.
    DeploymentSucceeded,
    /// A deployment to an environment failed.
    DeploymentFailed,
    /// A temporary environment is approaching its TTL expiry.
    TempEnvExpiryWarning,
    /// A temporary environment has expired.
    TempEnvExpired,
}

impl NotificationEvent {
    /// Return the email template name used by the email provider.
    pub fn template_name(&self) -> &'static str {
        match self {
            Self::ChangesetSubmitted => "changeset_submitted",
            Self::ReviewRequested => "review_requested",
            Self::ChangesetApproved => "changeset_approved",
            Self::ChangesRequested => "changes_requested",
            Self::ChangesetRejected => "changeset_rejected",
            Self::ChangesetQueued => "changeset_queued",
            Self::ReleaseCreated => "release_created",
            Self::ReleasePublished => "release_published",
            Self::DeploymentStarted => "deployment_started",
            Self::DeploymentSucceeded => "deployment_succeeded",
            Self::DeploymentFailed => "deployment_failed",
            Self::TempEnvExpiryWarning => "temp_env_expiry_warning",
            Self::TempEnvExpired => "temp_env_expired",
        }
    }
}
```

### 3.3 `conman-core/src/models/email.rs`

```rust
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Payload for a single outbound email. Constructed by the notification
/// service and handed off to the email provider for delivery.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmailPayload {
    /// Recipient email address.
    pub to: String,

    /// Email subject line.
    pub subject: String,

    /// Template identifier recognized by the email provider.
    pub template_name: String,

    /// Key-value data interpolated into the template (e.g., changeset title,
    /// app name, actor name, link URL).
    pub template_data: HashMap<String, serde_json::Value>,
}
```

### 3.4 `conman-core/src/services/notification_service.rs`

```rust
use crate::models::audit::RequestContext;
use crate::models::email::EmailPayload;
use crate::models::notification::{NotificationEvent, NotificationPreference};
use crate::ConmanError;

/// Orchestrates notification delivery for all system events.
///
/// For each event, the service determines recipients, checks their notification
/// preferences, builds email payloads, and dispatches them for delivery.
pub struct NotificationService {
    /// Repository for notification preferences.
    notification_pref_repo: NotificationPrefRepo,

    /// Repository for app memberships (used to resolve recipients by role).
    membership_repo: AppMembershipRepo,

    /// Repository for user records (used to look up email addresses).
    user_repo: UserRepo,

    /// Email sending backend (trait object for testability).
    email_sender: Box<dyn EmailSender>,
}

/// Trait abstracting the email delivery backend. Implementations may use
/// an SMTP relay, a transactional email API (SendGrid, SES, etc.), or a
/// no-op sender for tests.
#[async_trait::async_trait]
pub trait EmailSender: Send + Sync {
    /// Send a single email. Returns Ok on accepted-for-delivery.
    /// Implementations should handle retries internally.
    async fn send(&self, payload: &EmailPayload) -> Result<(), ConmanError>;
}

/// Contextual data passed alongside every notification dispatch, used to
/// resolve recipients and build template data.
pub struct NotificationContext {
    /// The app where the event occurred.
    pub app_id: String,
    pub app_name: String,

    /// The user who triggered the event (None for system events).
    pub actor_user_id: Option<String>,
    pub actor_name: Option<String>,

    /// Entity-specific fields for template interpolation.
    pub entity_id: String,
    pub entity_title: Option<String>,

    /// Deep link URL for the notification (e.g., changeset detail page).
    pub link_url: Option<String>,

    /// Additional template data specific to the event type.
    pub extra: std::collections::HashMap<String, serde_json::Value>,
}

impl NotificationService {
    /// Dispatch a notification for the given event.
    ///
    /// 1. Resolve the set of recipient user IDs based on event type and context.
    /// 2. Filter out users who have email_enabled = false.
    /// 3. Filter out the actor (users do not receive notifications for their
    ///    own actions).
    /// 4. Build an EmailPayload per recipient.
    /// 5. Send each email (fire-and-forget; failures are logged, not propagated).
    pub async fn notify(
        &self,
        event: NotificationEvent,
        ctx: &NotificationContext,
    ) -> Result<(), ConmanError> {
        // Determine who should receive this notification.
        let recipient_user_ids = self.resolve_recipients(&event, ctx).await?;

        // Load preferences for all candidate recipients in one query.
        let prefs = self
            .notification_pref_repo
            .find_by_user_ids(&recipient_user_ids)
            .await?;

        // Build a lookup set of users who have opted out.
        let opted_out: std::collections::HashSet<String> = prefs
            .iter()
            .filter(|p| !p.email_enabled)
            .map(|p| p.user_id.clone())
            .collect();

        // Filter: remove opted-out users and the actor themselves.
        let final_recipients: Vec<String> = recipient_user_ids
            .into_iter()
            .filter(|uid| !opted_out.contains(uid))
            .filter(|uid| ctx.actor_user_id.as_ref() != Some(uid))
            .collect();

        // Bail early if no one to notify.
        if final_recipients.is_empty() {
            return Ok(());
        }

        // Look up email addresses for the remaining recipients.
        let users = self.user_repo.find_by_ids(&final_recipients).await?;

        // Build and send one email per recipient.
        let subject = self.build_subject(&event, ctx);
        let template_name = event.template_name().to_string();

        for user in &users {
            let payload = EmailPayload {
                to: user.email.clone(),
                subject: subject.clone(),
                template_name: template_name.clone(),
                template_data: self.build_template_data(&event, ctx, user),
            };

            // Fire-and-forget: log errors but do not fail the caller.
            if let Err(e) = self.email_sender.send(&payload).await {
                tracing::error!(
                    error = %e,
                    to = %user.email,
                    event = ?event,
                    "failed to send notification email"
                );
            }
        }

        Ok(())
    }

    /// Resolve recipient user IDs for a given event type.
    ///
    /// Recipient rules by event:
    /// - ChangesetSubmitted: all reviewers + config_managers + app_admins
    /// - ReviewRequested: the specifically requested reviewer
    /// - ChangesetApproved: changeset author
    /// - ChangesRequested: changeset author
    /// - ChangesetRejected: changeset author
    /// - ChangesetQueued: changeset author + config_managers + app_admins
    /// - ReleaseCreated: all config_managers + app_admins
    /// - ReleasePublished: all app members
    /// - DeploymentStarted: all config_managers + app_admins
    /// - DeploymentSucceeded: all app members
    /// - DeploymentFailed: all config_managers + app_admins
    /// - TempEnvExpiryWarning: temp env creator
    /// - TempEnvExpired: temp env creator
    async fn resolve_recipients(
        &self,
        event: &NotificationEvent,
        ctx: &NotificationContext,
    ) -> Result<Vec<String>, ConmanError> {
        match event {
            NotificationEvent::ChangesetSubmitted => {
                self.membership_repo
                    .find_users_with_roles(
                        &ctx.app_id,
                        &["reviewer", "config_manager", "app_admin"],
                    )
                    .await
            }
            NotificationEvent::ReviewRequested => {
                // The requested reviewer's ID is passed in extra.reviewer_user_id.
                if let Some(reviewer_id) = ctx.extra.get("reviewer_user_id") {
                    Ok(vec![reviewer_id
                        .as_str()
                        .unwrap_or_default()
                        .to_string()])
                } else {
                    Ok(vec![])
                }
            }
            NotificationEvent::ChangesetApproved
            | NotificationEvent::ChangesRequested
            | NotificationEvent::ChangesetRejected => {
                // Notify the changeset author.
                if let Some(author_id) = ctx.extra.get("author_user_id") {
                    Ok(vec![author_id
                        .as_str()
                        .unwrap_or_default()
                        .to_string()])
                } else {
                    Ok(vec![])
                }
            }
            NotificationEvent::ChangesetQueued => {
                let mut recipients = self
                    .membership_repo
                    .find_users_with_roles(
                        &ctx.app_id,
                        &["config_manager", "app_admin"],
                    )
                    .await?;
                if let Some(author_id) = ctx.extra.get("author_user_id") {
                    let author = author_id.as_str().unwrap_or_default().to_string();
                    if !recipients.contains(&author) {
                        recipients.push(author);
                    }
                }
                Ok(recipients)
            }
            NotificationEvent::ReleaseCreated
            | NotificationEvent::DeploymentStarted
            | NotificationEvent::DeploymentFailed => {
                self.membership_repo
                    .find_users_with_roles(
                        &ctx.app_id,
                        &["config_manager", "app_admin"],
                    )
                    .await
            }
            NotificationEvent::ReleasePublished
            | NotificationEvent::DeploymentSucceeded => {
                self.membership_repo
                    .find_all_members(&ctx.app_id)
                    .await
            }
            NotificationEvent::TempEnvExpiryWarning
            | NotificationEvent::TempEnvExpired => {
                if let Some(creator_id) = ctx.extra.get("creator_user_id") {
                    Ok(vec![creator_id
                        .as_str()
                        .unwrap_or_default()
                        .to_string()])
                } else {
                    Ok(vec![])
                }
            }
        }
    }

    /// Build a human-readable subject line for the email.
    fn build_subject(
        &self,
        event: &NotificationEvent,
        ctx: &NotificationContext,
    ) -> String {
        let app = &ctx.app_name;
        let title = ctx.entity_title.as_deref().unwrap_or(&ctx.entity_id);
        match event {
            NotificationEvent::ChangesetSubmitted => {
                format!("[{app}] Changeset submitted: {title}")
            }
            NotificationEvent::ReviewRequested => {
                format!("[{app}] Review requested: {title}")
            }
            NotificationEvent::ChangesetApproved => {
                format!("[{app}] Changeset approved: {title}")
            }
            NotificationEvent::ChangesRequested => {
                format!("[{app}] Changes requested: {title}")
            }
            NotificationEvent::ChangesetRejected => {
                format!("[{app}] Changeset rejected: {title}")
            }
            NotificationEvent::ChangesetQueued => {
                format!("[{app}] Changeset queued: {title}")
            }
            NotificationEvent::ReleaseCreated => {
                format!("[{app}] Release created: {title}")
            }
            NotificationEvent::ReleasePublished => {
                format!("[{app}] Release published: {title}")
            }
            NotificationEvent::DeploymentStarted => {
                format!("[{app}] Deployment started: {title}")
            }
            NotificationEvent::DeploymentSucceeded => {
                format!("[{app}] Deployment succeeded: {title}")
            }
            NotificationEvent::DeploymentFailed => {
                format!("[{app}] Deployment failed: {title}")
            }
            NotificationEvent::TempEnvExpiryWarning => {
                format!("[{app}] Temp environment expiring soon: {title}")
            }
            NotificationEvent::TempEnvExpired => {
                format!("[{app}] Temp environment expired: {title}")
            }
        }
    }

    /// Build template data HashMap for a specific recipient.
    fn build_template_data(
        &self,
        event: &NotificationEvent,
        ctx: &NotificationContext,
        _recipient: &User,
    ) -> std::collections::HashMap<String, serde_json::Value> {
        let mut data = ctx.extra.clone();
        data.insert("app_name".to_string(), serde_json::json!(ctx.app_name));
        data.insert("entity_id".to_string(), serde_json::json!(ctx.entity_id));
        if let Some(title) = &ctx.entity_title {
            data.insert("entity_title".to_string(), serde_json::json!(title));
        }
        if let Some(actor) = &ctx.actor_name {
            data.insert("actor_name".to_string(), serde_json::json!(actor));
        }
        if let Some(link) = &ctx.link_url {
            data.insert("link_url".to_string(), serde_json::json!(link));
        }
        data.insert(
            "event_type".to_string(),
            serde_json::json!(event.template_name()),
        );
        data
    }
}
```

### 3.5 `conman-db/src/repos/audit_repo.rs`

```rust
use chrono::Utc;
use mongodb::bson::{self, doc, oid::ObjectId, Document};
use mongodb::{Collection, Database, IndexModel};
use mongodb::options::IndexOptions;

use conman_core::models::audit::AuditEvent;
use conman_core::ConmanError;

/// Repository for the append-only `audit_events` collection.
///
/// This repository intentionally exposes only insert and query operations.
/// There are no update or delete methods — audit events are immutable once
/// written.
pub struct AuditRepo {
    collection: Collection<Document>,
}

impl AuditRepo {
    pub fn new(db: &Database) -> Self {
        Self {
            collection: db.collection("audit_events"),
        }
    }

    /// Insert an audit event into the collection.
    ///
    /// This is the only write operation. Audit events are fire-and-forget
    /// from the caller's perspective: errors are logged but should not block
    /// the originating request.
    pub async fn emit(&self, event: AuditEvent) -> Result<(), ConmanError> {
        let mut doc = bson::to_document(&event).map_err(|e| ConmanError::Internal {
            message: format!("failed to serialize audit event: {e}"),
        })?;

        // Let MongoDB assign _id automatically.
        doc.remove("id");

        self.collection
            .insert_one(doc)
            .await
            .map_err(|e| ConmanError::Internal {
                message: format!("failed to insert audit event: {e}"),
            })?;

        Ok(())
    }

    /// Query audit events for an app with optional filters and pagination.
    ///
    /// Supports filtering by entity_type, entity_id, action, and
    /// actor_user_id. Results are ordered by occurred_at descending
    /// (most recent first).
    pub async fn query(
        &self,
        app_id: &str,
        entity_type: Option<&str>,
        entity_id: Option<&str>,
        action: Option<&str>,
        actor_user_id: Option<&str>,
        skip: u64,
        limit: i64,
    ) -> Result<(Vec<AuditEvent>, u64), ConmanError> {
        let mut filter = doc! { "app_id": app_id };

        if let Some(et) = entity_type {
            filter.insert("entity_type", et);
        }
        if let Some(eid) = entity_id {
            filter.insert("entity_id", eid);
        }
        if let Some(a) = action {
            filter.insert("action", a);
        }
        if let Some(uid) = actor_user_id {
            filter.insert("actor_user_id", uid);
        }

        // Count total matching documents for pagination.
        let total = self
            .collection
            .count_documents(filter.clone())
            .await
            .map_err(|e| ConmanError::Internal {
                message: format!("audit query count failed: {e}"),
            })?;

        // Fetch the page of results, sorted by occurred_at descending.
        let options = mongodb::options::FindOptions::builder()
            .sort(doc! { "occurred_at": -1 })
            .skip(skip)
            .limit(limit)
            .build();

        let mut cursor = self
            .collection
            .find(filter)
            .with_options(options)
            .await
            .map_err(|e| ConmanError::Internal {
                message: format!("audit query failed: {e}"),
            })?;

        let mut events = Vec::new();
        while cursor.advance().await.map_err(|e| ConmanError::Internal {
            message: format!("audit cursor error: {e}"),
        })? {
            let doc = cursor.deserialize_current().map_err(|e| ConmanError::Internal {
                message: format!("audit event deserialization failed: {e}"),
            })?;
            let event: AuditEvent =
                bson::from_document(doc).map_err(|e| ConmanError::Internal {
                    message: format!("audit event conversion failed: {e}"),
                })?;
            events.push(event);
        }

        Ok((events, total))
    }
}

#[async_trait::async_trait]
impl EnsureIndexes for AuditRepo {
    async fn ensure_indexes(&self) -> Result<(), ConmanError> {
        let indexes = vec![
            // Primary query pattern: audit events for a specific entity within an app.
            IndexModel::builder()
                .keys(doc! {
                    "app_id": 1,
                    "entity_type": 1,
                    "entity_id": 1,
                    "occurred_at": -1,
                })
                .build(),

            // Query by actor across an app (e.g., "what did this user do?").
            IndexModel::builder()
                .keys(doc! { "actor_user_id": 1, "occurred_at": -1 })
                .build(),

            // Time-range queries (e.g., "all events in the last 24h").
            IndexModel::builder()
                .keys(doc! { "occurred_at": -1 })
                .build(),

            // Filter by app + action (e.g., "all deployments in this app").
            IndexModel::builder()
                .keys(doc! { "app_id": 1, "action": 1, "occurred_at": -1 })
                .build(),
        ];

        self.collection
            .create_indexes(indexes)
            .await
            .map_err(|e| ConmanError::Internal {
                message: format!("failed to create audit_events indexes: {e}"),
            })?;

        Ok(())
    }
}
```

### 3.6 `conman-db/src/repos/notification_pref_repo.rs`

```rust
use mongodb::bson::{self, doc, oid::ObjectId, Document};
use mongodb::{Collection, Database, IndexModel};
use mongodb::options::IndexOptions;

use conman_core::models::notification::NotificationPreference;
use conman_core::ConmanError;

/// Repository for the `notification_preferences` collection.
pub struct NotificationPrefRepo {
    collection: Collection<Document>,
}

impl NotificationPrefRepo {
    pub fn new(db: &Database) -> Self {
        Self {
            collection: db.collection("notification_preferences"),
        }
    }

    /// Get notification preferences for a user. If no preference document
    /// exists, returns a default preference with email_enabled = true.
    pub async fn get_or_default(
        &self,
        user_id: &str,
    ) -> Result<NotificationPreference, ConmanError> {
        let filter = doc! { "user_id": user_id };
        let result = self
            .collection
            .find_one(filter)
            .await
            .map_err(|e| ConmanError::Internal {
                message: format!("failed to query notification preferences: {e}"),
            })?;

        match result {
            Some(doc) => {
                bson::from_document(doc).map_err(|e| ConmanError::Internal {
                    message: format!("notification preference deserialization failed: {e}"),
                })
            }
            None => Ok(NotificationPreference::default_for_user(
                user_id.to_string(),
            )),
        }
    }

    /// Upsert notification preferences for a user. Creates the document if it
    /// does not exist, updates it otherwise.
    pub async fn upsert(
        &self,
        pref: &NotificationPreference,
    ) -> Result<NotificationPreference, ConmanError> {
        let filter = doc! { "user_id": &pref.user_id };
        let update = doc! {
            "$set": {
                "email_enabled": pref.email_enabled,
                "updated_at": bson::DateTime::from_chrono(pref.updated_at),
            },
            "$setOnInsert": {
                "user_id": &pref.user_id,
                "created_at": bson::DateTime::from_chrono(pref.created_at),
            },
        };

        let options = mongodb::options::UpdateOptions::builder()
            .upsert(true)
            .build();

        self.collection
            .update_one(filter, update)
            .with_options(options)
            .await
            .map_err(|e| ConmanError::Internal {
                message: format!("failed to upsert notification preference: {e}"),
            })?;

        self.get_or_default(&pref.user_id).await
    }

    /// Batch-fetch notification preferences for a list of user IDs.
    /// Users without a preference document are not included in the result
    /// (they are treated as having the default: email_enabled = true).
    pub async fn find_by_user_ids(
        &self,
        user_ids: &[String],
    ) -> Result<Vec<NotificationPreference>, ConmanError> {
        let filter = doc! {
            "user_id": { "$in": user_ids }
        };

        let mut cursor = self
            .collection
            .find(filter)
            .await
            .map_err(|e| ConmanError::Internal {
                message: format!("failed to query notification preferences: {e}"),
            })?;

        let mut prefs = Vec::new();
        while cursor.advance().await.map_err(|e| ConmanError::Internal {
            message: format!("notification pref cursor error: {e}"),
        })? {
            let doc = cursor.deserialize_current().map_err(|e| ConmanError::Internal {
                message: format!("notification pref deserialization failed: {e}"),
            })?;
            let pref: NotificationPreference =
                bson::from_document(doc).map_err(|e| ConmanError::Internal {
                    message: format!("notification pref conversion failed: {e}"),
                })?;
            prefs.push(pref);
        }

        Ok(prefs)
    }
}

#[async_trait::async_trait]
impl EnsureIndexes for NotificationPrefRepo {
    async fn ensure_indexes(&self) -> Result<(), ConmanError> {
        let indexes = vec![
            // One preference document per user.
            IndexModel::builder()
                .keys(doc! { "user_id": 1 })
                .options(IndexOptions::builder().unique(true).build())
                .build(),
        ];

        self.collection
            .create_indexes(indexes)
            .await
            .map_err(|e| ConmanError::Internal {
                message: format!(
                    "failed to create notification_preferences indexes: {e}"
                ),
            })?;

        Ok(())
    }
}
```

---

## 4. Database

### Collection: `audit_events`

Append-only. No update or delete operations are permitted on this collection.

| Field | Type | Description |
|-------|------|-------------|
| `_id` | `ObjectId` | Primary key (auto-assigned) |
| `occurred_at` | `DateTime` | BSON DateTime, when the action happened |
| `actor_user_id` | `String?` | References `users._id` hex; null for system actions |
| `app_id` | `String?` | References `apps._id` hex; null for cross-app actions |
| `entity_type` | `String` | Category of affected entity |
| `entity_id` | `String` | ID of the affected entity |
| `action` | `String` | What was done (see section 6.3 for full list) |
| `before` | `Document?` | Entity state snapshot before the action |
| `after` | `Document?` | Entity state snapshot after the action |
| `git_sha` | `String?` | Associated Git commit SHA, if applicable |
| `context.ip` | `String?` | Client IP address |
| `context.user_agent` | `String?` | User-Agent header |
| `context.request_id` | `String` | Request ID for log correlation |

**Indexes:**

```javascript
// Entity-scoped query: "all audit events for changeset X in app Y"
{ "app_id": 1, "entity_type": 1, "entity_id": 1, "occurred_at": -1 }

// Actor query: "what did this user do recently?"
{ "actor_user_id": 1, "occurred_at": -1 }

// Time-range scan: "all events in the last hour"
{ "occurred_at": -1 }

// Action filter: "all deployment events in app Y"
{ "app_id": 1, "action": 1, "occurred_at": -1 }
```

**Example document:**

```json
{
  "_id": ObjectId("665a1b2c3d4e5f6a70b09040"),
  "occurred_at": ISODate("2025-07-15T14:32:10Z"),
  "actor_user_id": "664f1a2b3c4d5e6f70809001",
  "app_id": "664f1a2b3c4d5e6f70809010",
  "entity_type": "changeset",
  "entity_id": "665a1b2c3d4e5f6a70b09030",
  "action": "submitted",
  "before": { "state": "draft" },
  "after": { "state": "submitted", "head_sha": "abc123def456" },
  "git_sha": "abc123def456",
  "context": {
    "ip": "10.0.1.42",
    "user_agent": "Mozilla/5.0 ...",
    "request_id": "req-550e8400-e29b-41d4-a716-446655440000"
  }
}
```

**Example document (system-initiated temp env expiration):**

```json
{
  "_id": ObjectId("665a1b2c3d4e5f6a70b09041"),
  "occurred_at": ISODate("2025-07-16T00:00:05Z"),
  "actor_user_id": null,
  "app_id": "664f1a2b3c4d5e6f70809010",
  "entity_type": "temp_environment",
  "entity_id": "665a1b2c3d4e5f6a70b09035",
  "action": "expired",
  "before": { "state": "active", "expires_at": "2025-07-16T00:00:00Z" },
  "after": { "state": "expired", "grace_until": "2025-07-16T01:00:00Z" },
  "git_sha": null,
  "context": {
    "ip": null,
    "user_agent": null,
    "request_id": "job-ttl-cleanup-20250716"
  }
}
```

### Collection: `notification_preferences`

| Field | Type | Description |
|-------|------|-------------|
| `_id` | `ObjectId` | Primary key |
| `user_id` | `String` | References `users._id` hex, unique |
| `email_enabled` | `bool` | Whether email notifications are on |
| `created_at` | `DateTime` | BSON DateTime |
| `updated_at` | `DateTime` | BSON DateTime |

**Indexes:**

```javascript
// One preference per user
{ "user_id": 1 }  // unique: true
```

**Example document:**

```json
{
  "_id": ObjectId("665a1b2c3d4e5f6a70b09050"),
  "user_id": "664f1a2b3c4d5e6f70809001",
  "email_enabled": true,
  "created_at": ISODate("2025-06-01T10:00:00Z"),
  "updated_at": ISODate("2025-07-10T08:30:00Z")
}
```

**Example document (opted out):**

```json
{
  "_id": ObjectId("665a1b2c3d4e5f6a70b09051"),
  "user_id": "664f1a2b3c4d5e6f70809002",
  "email_enabled": false,
  "created_at": ISODate("2025-06-02T14:30:00Z"),
  "updated_at": ISODate("2025-06-15T09:00:00Z")
}
```

---

## 5. API Endpoints

### 5.1 `GET /api/me/notification-preferences`

Retrieve the authenticated user's notification preferences.

| Attribute | Value |
|-----------|-------|
| Auth | Any authenticated user |
| RBAC | None (user can only see their own preferences) |

**Response 200:**

```json
{
  "data": {
    "user_id": "664f1a2b3c4d5e6f70809001",
    "email_enabled": true,
    "created_at": "2025-06-01T10:00:00Z",
    "updated_at": "2025-07-10T08:30:00Z"
  }
}
```

If the user has never updated their preferences, a default object is returned
with `email_enabled: true` and timestamps set to now.

**Errors:**

| Status | Code | Condition |
|--------|------|-----------|
| 401 | `unauthorized` | Missing or invalid JWT |

---

### 5.2 `PATCH /api/me/notification-preferences`

Update the authenticated user's notification preferences.

| Attribute | Value |
|-----------|-------|
| Auth | Any authenticated user |
| RBAC | None (user can only modify their own preferences) |

**Request body:**

```json
{
  "email_enabled": false
}
```

**Validation:**

- `email_enabled`: required, must be a boolean.

**Response 200:**

```json
{
  "data": {
    "user_id": "664f1a2b3c4d5e6f70809001",
    "email_enabled": false,
    "created_at": "2025-06-01T10:00:00Z",
    "updated_at": "2025-07-15T14:00:00Z"
  }
}
```

**Side effects:**

1. Upsert the preference document (creates if first update).
2. Emit audit event: `notification_preference.updated` with before/after.

**Errors:**

| Status | Code | Condition |
|--------|------|-----------|
| 400 | `validation_error` | Missing or non-boolean `email_enabled` |
| 401 | `unauthorized` | Missing or invalid JWT |

---

### 5.3 `GET /api/apps/:appId/audit?page=&limit=&entity_type=&entity_id=&action=&actor_user_id=`

Read-only paginated query of the audit log for an app.

| Attribute | Value |
|-----------|-------|
| Auth | Authenticated user |
| RBAC | `app_admin` on this app |
| Query params | `page` (default 1), `limit` (default 20, max 100), `entity_type` (optional), `entity_id` (optional), `action` (optional), `actor_user_id` (optional) |

**Response 200:**

```json
{
  "data": [
    {
      "id": "665a1b2c3d4e5f6a70b09040",
      "occurred_at": "2025-07-15T14:32:10Z",
      "actor_user_id": "664f1a2b3c4d5e6f70809001",
      "app_id": "664f1a2b3c4d5e6f70809010",
      "entity_type": "changeset",
      "entity_id": "665a1b2c3d4e5f6a70b09030",
      "action": "submitted",
      "before": { "state": "draft" },
      "after": { "state": "submitted", "head_sha": "abc123def456" },
      "git_sha": "abc123def456",
      "context": {
        "ip": "10.0.1.42",
        "user_agent": "Mozilla/5.0 ...",
        "request_id": "req-550e8400-e29b-41d4-a716-446655440000"
      }
    }
  ],
  "pagination": { "page": 1, "limit": 20, "total": 142 }
}
```

**Errors:**

| Status | Code | Condition |
|--------|------|-----------|
| 401 | `unauthorized` | Missing or invalid JWT |
| 403 | `forbidden` | Caller is not `app_admin` on this app |
| 404 | `not_found` | App does not exist |

---

## 6. Business Logic

### 6.1 Notification Fanout

For each `NotificationEvent`, the notification service determines recipients,
filters by preference and actor exclusion, and sends emails.

**Recipient resolution by event:**

| Event | Recipients |
|-------|-----------|
| `ChangesetSubmitted` | All users with `reviewer`, `config_manager`, or `app_admin` role in the app |
| `ReviewRequested` | The specifically requested reviewer |
| `ChangesetApproved` | Changeset author |
| `ChangesRequested` | Changeset author |
| `ChangesetRejected` | Changeset author |
| `ChangesetQueued` | Changeset author + all `config_manager` + `app_admin` users |
| `ReleaseCreated` | All `config_manager` + `app_admin` users |
| `ReleasePublished` | All app members |
| `DeploymentStarted` | All `config_manager` + `app_admin` users |
| `DeploymentSucceeded` | All app members |
| `DeploymentFailed` | All `config_manager` + `app_admin` users |
| `TempEnvExpiryWarning` | Temp environment creator |
| `TempEnvExpired` | Temp environment creator |

**Filtering rules applied to every event:**

1. Remove users who have `email_enabled = false` in their notification preferences.
2. Remove the actor who triggered the event (no self-notifications).
3. Users with no preference document are treated as opted-in (default `email_enabled: true`).

### 6.2 Email Sending

Email delivery uses a trait-based provider abstraction (`EmailSender`). In v1
the concrete implementation is configurable at startup:

- **Production:** Transactional email API (SendGrid, AWS SES, or similar).
  Configured via `CONMAN_EMAIL_PROVIDER` and `CONMAN_EMAIL_API_KEY` environment
  variables.
- **Development/test:** A no-op sender that logs payloads to tracing at `debug`
  level.

**Delivery semantics:**

- Emails are sent inline (not queued as jobs) to keep the implementation simple
  in v1. The `EmailSender::send` method is async and may perform internal retries.
- Notification dispatch is fire-and-forget from the caller's perspective. The
  `NotificationService::notify` method logs errors but does not propagate them
  to the originating request handler.
- If the email provider is temporarily unreachable, the notification is lost.
  This is acceptable in v1; a job-based retry queue can be added in a future
  iteration.

**Email templates:**

Each `NotificationEvent` variant maps to a named template. Templates contain:

- App name and entity title in the subject line.
- Actor name (who performed the action).
- A summary of what happened.
- A deep link to the relevant page in the UI.
- Unsubscribe hint (pointing to notification preferences).

Template rendering is handled by the email provider (server-side templates in
SendGrid/SES). Conman sends structured `template_data` key-value pairs.

### 6.3 Auditable Actions (Complete Enumeration)

Every mutation handler across the system emits an audit event. The following is
the complete list of `entity_type` + `action` pairs, grouped by domain area.
Each handler is responsible for calling `audit_repo.emit()` after a successful
mutation.

**Workspace lifecycle:**

| entity_type | action | Handler | Emitting epic |
|-------------|--------|---------|---------------|
| `workspace` | `created` | `POST /api/apps/:appId/workspaces` | E04 |
| `workspace` | `updated` | `PATCH /api/apps/:appId/workspaces/:workspaceId` | E04 |
| `workspace` | `reset` | `POST .../workspaces/:workspaceId/reset` | E04 |
| `workspace` | `synced_integration` | `POST .../workspaces/:workspaceId/sync-integration` | E04 |

**File operations:**

| entity_type | action | Handler | Emitting epic |
|-------------|--------|---------|---------------|
| `file` | `written` | `PUT .../workspaces/:workspaceId/files` | E04 |
| `file` | `deleted` | `DELETE .../workspaces/:workspaceId/files` | E04 |
| `file` | `checkpoint` | `POST .../workspaces/:workspaceId/checkpoints` | E04 |

**Changeset lifecycle:**

| entity_type | action | Handler | Emitting epic |
|-------------|--------|---------|---------------|
| `changeset` | `created` | `POST /api/apps/:appId/changesets` | E05 |
| `changeset` | `updated` | `PATCH .../changesets/:changesetId` | E05 |
| `changeset` | `submitted` | `POST .../changesets/:changesetId/submit` | E05 |
| `changeset` | `resubmitted` | `POST .../changesets/:changesetId/resubmit` | E05 |
| `changeset` | `approved` | `POST .../changesets/:changesetId/review` (verdict=approve) | E05 |
| `changeset` | `changes_requested` | `POST .../changesets/:changesetId/review` (verdict=changes_requested) | E05 |
| `changeset` | `rejected` | `POST .../changesets/:changesetId/review` (verdict=reject) | E05 |
| `changeset` | `queued` | `POST .../changesets/:changesetId/queue` | E07 |
| `changeset` | `moved_to_draft` | `POST .../changesets/:changesetId/move-to-draft` | E05 |
| `changeset` | `conflicted` | Revalidation worker | E07 |
| `changeset` | `needs_revalidation` | Revalidation worker | E07 |

**Comments:**

| entity_type | action | Handler | Emitting epic |
|-------------|--------|---------|---------------|
| `comment` | `created` | `POST .../changesets/:changesetId/comments` | E05 |
| `comment` | `edited` | `PATCH .../changesets/:changesetId/comments/:commentId` | E05 |

**Release lifecycle:**

| entity_type | action | Handler | Emitting epic |
|-------------|--------|---------|---------------|
| `release` | `created` | `POST /api/apps/:appId/releases` | E08 |
| `release` | `changesets_modified` | `POST .../releases/:releaseId/changesets` | E08 |
| `release` | `reordered` | `POST .../releases/:releaseId/reorder` | E08 |
| `release` | `assembled` | `POST .../releases/:releaseId/assemble` | E08 |
| `release` | `published` | `POST .../releases/:releaseId/publish` | E08 |

**Deployment lifecycle:**

| entity_type | action | Handler | Emitting epic |
|-------------|--------|---------|---------------|
| `deployment` | `started` | `POST .../environments/:envId/deploy` | E09 |
| `deployment` | `succeeded` | Deploy worker on completion | E09 |
| `deployment` | `failed` | Deploy worker on failure | E09 |
| `deployment` | `promoted` | `POST .../environments/:envId/promote` | E09 |
| `deployment` | `rolled_back` | `POST .../environments/:envId/rollback` | E09 |

**Temp environment lifecycle:**

| entity_type | action | Handler | Emitting epic |
|-------------|--------|---------|---------------|
| `temp_environment` | `created` | `POST /api/apps/:appId/temp-envs` | E10 |
| `temp_environment` | `extended` | `POST .../temp-envs/:tempEnvId/extend` | E10 |
| `temp_environment` | `expired` | TTL cleanup job | E10 |
| `temp_environment` | `undo_expired` | `POST .../temp-envs/:tempEnvId/undo-expire` | E10 |
| `temp_environment` | `deleted` | `DELETE .../temp-envs/:tempEnvId` | E10 |

**App and settings:**

| entity_type | action | Handler | Emitting epic |
|-------------|--------|---------|---------------|
| `app` | `created` | `POST /api/apps` | E03 |
| `app` | `settings_updated` | `PATCH /api/apps/:appId/settings` | E03 |
| `app` | `environments_updated` | `PATCH /api/apps/:appId/environments` | E03 |

**Membership and invites:**

| entity_type | action | Handler | Emitting epic |
|-------------|--------|---------|---------------|
| `invite` | `created` | `POST /api/apps/:appId/invites` | E02 |
| `invite` | `resent` | `POST .../invites/:inviteId/resend` | E02 |
| `invite` | `revoked` | `DELETE .../invites/:inviteId` | E02 |
| `invite` | `accepted` | `POST /api/auth/accept-invite` | E02 |
| `membership` | `role_changed` | Role assignment handler | E02 |

**Notification preferences:**

| entity_type | action | Handler | Emitting epic |
|-------------|--------|---------|---------------|
| `notification_preference` | `updated` | `PATCH /api/me/notification-preferences` | E11 |

### 6.4 Audit Retention

Audit events are retained forever in v1. There is no TTL, no archival, and no
purge mechanism. The `audit_events` collection grows unbounded.

### 6.5 Audit Immutability

The `AuditRepo` struct intentionally exposes only two public methods: `emit`
(insert) and `query` (read). There are no `update` or `delete` methods. Code
review should enforce that no other code path writes to the `audit_events`
collection outside of `AuditRepo::emit`.

### 6.6 Audit Write Semantics

Audit writes are fire-and-forget. Each handler calls `audit_repo.emit()`
after a successful mutation. If the audit write fails:

- The failure is logged at `error` level via tracing.
- The originating request is **not** failed or rolled back.
- This trade-off prioritizes availability over audit completeness for
  transient MongoDB failures. In practice, MongoDB insert failures on a
  healthy cluster are extremely rare.

### 6.7 Backfill for Legacy Transitions

If prior epics (E02-E10) were implemented before E11, some handlers may not
yet emit audit events. E11-05 covers a backfill sweep:

1. Enumerate every mutation handler in the codebase (using the table in
   section 6.3 as the checklist).
2. For each handler, verify an `audit_repo.emit()` call is present.
3. Add missing audit calls where absent.
4. Write integration tests asserting audit event emission for each handler.

---

## 7. Gitaly-rs Integration

N/A. This epic does not introduce any new Git operations. All data is sourced
from MongoDB (audit events and notification preferences) and the email provider.

---

## 8. Implementation Checklist

### E11-01: Email templates and provider integration

- [ ] Define `EmailSender` trait in `conman-core/src/services/email.rs`
- [ ] Implement `NoopEmailSender` for development/testing (logs payloads at debug level)
- [ ] Implement `ApiEmailSender` for production (configurable provider via `CONMAN_EMAIL_PROVIDER`)
- [ ] Add `CONMAN_EMAIL_PROVIDER`, `CONMAN_EMAIL_API_KEY`, `CONMAN_EMAIL_FROM_ADDRESS` environment variables to `Config`
- [ ] Create `EmailPayload` struct in `conman-core/src/models/email.rs`
- [ ] Define email template names for all 13 notification events
- [ ] Write unit test: `NoopEmailSender` returns `Ok(())` and does not panic
- [ ] Write unit test: `EmailPayload` serializes correctly to JSON

### E11-02: Per-user notification preferences

- [ ] Add `NotificationPreference` struct to `conman-core/src/models/notification.rs`
- [ ] Add `NotificationPrefRepo` to `conman-db` with `ensure_indexes()` (unique `user_id`)
- [ ] Implement `get_or_default()`: returns stored preference or default (email_enabled=true)
- [ ] Implement `upsert()`: creates or updates preference document
- [ ] Implement `find_by_user_ids()`: batch fetch for fanout filtering
- [ ] Add `GET /api/me/notification-preferences` handler in `conman-api`
- [ ] Add `PATCH /api/me/notification-preferences` handler in `conman-api`
- [ ] Emit audit event on preference update
- [ ] Write unit test: `NotificationPreference::default_for_user` sets email_enabled=true
- [ ] Write integration test: get default preferences (no document), returns email_enabled=true
- [ ] Write integration test: update preferences, read back, verify toggle state
- [ ] Write integration test: PATCH with non-boolean value returns 400

### E11-03: Event fanout for required notifications

- [ ] Add `NotificationEvent` enum to `conman-core/src/models/notification.rs`
- [ ] Add `NotificationContext` struct
- [ ] Implement `NotificationService` with `notify()` method
- [ ] Implement `resolve_recipients()` for all 13 event types
- [ ] Implement `build_subject()` for all 13 event types
- [ ] Implement `build_template_data()` with common and event-specific fields
- [ ] Integrate `NotificationService` into `AppState` for handler access
- [ ] Add `notify()` calls to changeset handlers: submit, review (approve/changes_requested/reject), queue
- [ ] Add `notify()` calls to release handlers: create, publish
- [ ] Add `notify()` calls to deployment handlers: start, succeed, fail
- [ ] Add `notify()` calls to temp env handlers: expiry warning (from TTL job), expired (from TTL job)
- [ ] Write unit test: `resolve_recipients` returns correct users for each event type
- [ ] Write unit test: actor is excluded from recipients
- [ ] Write unit test: opted-out users are excluded from recipients
- [ ] Write unit test: users with no preference document are included (default opt-in)
- [ ] Write integration test: submit changeset triggers notification to reviewers

### E11-04: Append-only audit event writer + schema enforcement

- [ ] Add `AuditEvent` and `RequestContext` structs to `conman-core/src/models/audit.rs`
- [ ] Add `AuditRepo` to `conman-db` with `ensure_indexes()`
- [ ] Implement `emit()`: insert-only, fire-and-forget error handling
- [ ] Implement `query()`: filtered, paginated, ordered by occurred_at desc
- [ ] Add `GET /api/apps/:appId/audit` handler with RBAC (`app_admin` only)
- [ ] Add pagination and filter support (entity_type, entity_id, action, actor_user_id)
- [ ] Verify `AuditRepo` has no public update or delete methods (code review gate)
- [ ] Write unit test: `AuditEvent` serializes to expected JSON shape
- [ ] Write integration test: emit audit event, query it back, verify all fields
- [ ] Write integration test: query with entity_type filter returns only matching events
- [ ] Write integration test: query with pagination returns correct page and total
- [ ] Write integration test: non-admin querying audit returns 403

### E11-05: Backfill audit for critical legacy transitions

- [ ] Audit every mutation handler against the complete enumeration in section 6.3
- [ ] Add missing `audit_repo.emit()` calls to any handler that lacks them
- [ ] For workspace handlers (E04): verify `created`, `updated`, `reset`, `synced_integration` emit audit
- [ ] For file handlers (E04): verify `written`, `deleted`, `checkpoint` emit audit
- [ ] For changeset handlers (E05): verify all state transitions emit audit
- [ ] For comment handlers (E05): verify `created`, `edited` emit audit
- [ ] For release handlers (E08): verify `created`, `changesets_modified`, `reordered`, `assembled`, `published` emit audit
- [ ] For deployment handlers (E09): verify `started`, `succeeded`, `failed`, `promoted`, `rolled_back` emit audit
- [ ] For temp env handlers (E10): verify `created`, `extended`, `expired`, `undo_expired`, `deleted` emit audit
- [ ] For invite/membership handlers (E02): verify `created`, `resent`, `revoked`, `accepted`, `role_changed` emit audit
- [ ] For app/settings handlers (E03): verify `created`, `settings_updated`, `environments_updated` emit audit
- [ ] Write integration test for each handler: perform mutation, assert audit event was emitted with correct entity_type and action

---

## 9. Test Cases

### Unit tests (conman-core)

| # | Test | Assertion |
|---|------|-----------|
| 1 | `NotificationPreference::default_for_user` | `email_enabled` is `true` |
| 2 | `NotificationEvent::template_name` for each variant | Returns the correct snake_case template name |
| 3 | `NotificationEvent` serde round-trip | Serializes and deserializes to same variant |
| 4 | `AuditEvent` serialization with all fields | JSON includes all fields with correct names |
| 5 | `AuditEvent` serialization with None fields | `before`, `after`, `git_sha` are null in JSON |
| 6 | `RequestContext` serialization | Includes `ip`, `user_agent`, `request_id` |
| 7 | `EmailPayload` serialization | JSON includes `to`, `subject`, `template_name`, `template_data` |
| 8 | `build_subject` for ChangesetSubmitted | Returns `"[app_name] Changeset submitted: title"` |
| 9 | `build_subject` for DeploymentFailed | Returns `"[app_name] Deployment failed: title"` |
| 10 | `resolve_recipients` for ChangesetSubmitted | Returns users with reviewer, config_manager, app_admin roles |
| 11 | `resolve_recipients` for ChangesetApproved | Returns changeset author only |
| 12 | `resolve_recipients` for ReleasePublished | Returns all app members |
| 13 | `resolve_recipients` for TempEnvExpired | Returns temp env creator only |
| 14 | Actor excluded from notification recipients | Actor user_id not in final recipient list |
| 15 | Opted-out user excluded from recipients | User with email_enabled=false not in final list |
| 16 | User with no preference document included | Treated as default opt-in |

### Integration tests

| # | Test | Setup | Assertion |
|---|------|-------|-----------|
| 17 | Get default notification preferences | New user, no preference doc | Returns 200 with `email_enabled: true` |
| 18 | Update notification preferences | User toggles `email_enabled: false` | Returns 200, subsequent GET shows `false` |
| 19 | Update preferences upserts on first call | New user, no existing doc | Document created with correct `user_id` |
| 20 | PATCH with missing email_enabled | Send `{}` | Returns 400 `validation_error` |
| 21 | PATCH with non-boolean email_enabled | Send `{ "email_enabled": "yes" }` | Returns 400 `validation_error` |
| 22 | Preference update emits audit event | Toggle preference | Audit event with `entity_type: notification_preference`, `action: updated` |
| 23 | Emit audit event | Insert event via `AuditRepo::emit` | Document exists in `audit_events` with all fields |
| 24 | Query audit events — no filters | 5 events in app | Returns all 5, ordered by `occurred_at` desc |
| 25 | Query audit events — entity_type filter | 3 changeset + 2 release events | Filter `entity_type=changeset` returns 3 |
| 26 | Query audit events — entity_id filter | Multiple events for different entities | Returns only events for specified entity_id |
| 27 | Query audit events — action filter | Mix of actions | Filter `action=submitted` returns only submitted events |
| 28 | Query audit events — pagination | 25 events, page=2, limit=10 | Returns events 11-20, total=25 |
| 29 | Query audit events — non-admin | User with `user` role | Returns 403 `forbidden` |
| 30 | Query audit events — unknown app | Non-existent app_id | Returns 404 `not_found` |
| 31 | Notification fanout — changeset submitted | Submit changeset, 2 reviewers + 1 admin | 2 emails sent (admin + 1 reviewer; other reviewer is actor) |
| 32 | Notification fanout — opted-out user | Submit changeset, 1 reviewer opted out | 0 emails to opted-out reviewer |
| 33 | Notification fanout — deployment failed | Deployment fails, 1 config_manager + 1 admin | 2 emails sent |
| 34 | Notification fanout — release published | Publish release, 5 app members | 4 emails sent (excluding actor) |
| 35 | Notification fanout — temp env expiry warning | TTL job fires warning | 1 email to temp env creator |
| 36 | Audit backfill — workspace created | Create workspace | Audit event: `entity_type=workspace, action=created` |
| 37 | Audit backfill — file written | Write file to workspace | Audit event: `entity_type=file, action=written` |
| 38 | Audit backfill — changeset submitted | Submit changeset | Audit event: `entity_type=changeset, action=submitted` |
| 39 | Audit backfill — release published | Publish release | Audit event: `entity_type=release, action=published` |
| 40 | Audit backfill — deployment started | Start deployment | Audit event: `entity_type=deployment, action=started` |
| 41 | Audit backfill — invite created | Create invite | Audit event: `entity_type=invite, action=created` |
| 42 | Audit backfill — settings updated | Update app settings | Audit event with `before`/`after` snapshots |
| 43 | AuditRepo has no update/delete methods | Compile-time check | `AuditRepo` exposes only `emit` and `query` public methods |

---

## 10. Acceptance Criteria

- [ ] All 13 scoped notification events (section 10 of scope doc) emit email
  notifications to the correct recipients when the user has `email_enabled: true`.
- [ ] Users are never notified of their own actions (actor exclusion).
- [ ] Users with `email_enabled: false` never receive email notifications.
- [ ] Users with no notification preference document receive notifications by
  default (opt-in).
- [ ] `GET /api/me/notification-preferences` returns the user's current preference
  or a default with `email_enabled: true`.
- [ ] `PATCH /api/me/notification-preferences` toggles the email preference and
  persists the change.
- [ ] `GET /api/apps/:appId/audit` returns a paginated, filterable, read-only
  audit log accessible only to `app_admin` users.
- [ ] Every mutation handler in the system (as enumerated in section 6.3) emits
  an `AuditEvent` with correct `entity_type`, `action`, and `before`/`after`
  snapshots.
- [ ] The `audit_events` collection is append-only: no update or delete
  operations exist in the codebase.
- [ ] Audit events include full `RequestContext` (IP, user agent, request ID)
  for traceability.
- [ ] Audit retention is unbounded (no TTL, no purge).
- [ ] Audit write failures are logged but do not block or fail the originating
  request.
- [ ] Email delivery failures are logged but do not block or fail the originating
  request.
- [ ] All endpoints follow the standard response envelope and error format.
- [ ] Pagination works correctly on the audit query endpoint.

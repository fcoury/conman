# E02 Auth, Invites, Memberships, RBAC

## 1. Goal

Secure all API access with local email/password authentication and enforce
per-app role-based access control so that every request is authenticated by
default and authorization is checked against the permission matrix from the
scope doc.

## 2. Dependencies

| Dependency | What it provides |
|------------|------------------|
| E00 Platform Foundation | Axum skeleton, MongoDB connection, error envelope, pagination, request tracing |

## 3. Rust Types

### Domain types (`conman-core`)

```rust
use chrono::{DateTime, Utc};
use mongodb::bson::oid::ObjectId;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// Role & capability enums
// ---------------------------------------------------------------------------

/// App-scoped role assigned via membership. Ordered by ascending privilege so
/// that comparison operators express "at least this role" checks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    User = 0,
    Reviewer = 1,
    ConfigManager = 2,
    AppAdmin = 3,
}

impl Role {
    /// Returns true when `self` is equal to or higher than `required`.
    pub fn satisfies(&self, required: Role) -> bool {
        *self >= required
    }
}

impl std::fmt::Display for Role {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Role::Member => write!(f, "user"),
            Role::Reviewer => write!(f, "reviewer"),
            Role::ConfigManager => write!(f, "config_manager"),
            Role::Admin => write!(f, "admin"),
        }
    }
}

/// Every guarded operation in the system. Handlers call
/// `auth_user.require_capability(repo_id, Capability::X)?` before proceeding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Capability {
    /// Read app/repo metadata, list workspaces, view changesets, etc.
    ReadApp,
    /// Create or edit the caller's own workspace.
    EditOwnWorkspace,
    /// Create or modify the caller's own changeset.
    EditOwnChangeset,
    /// Submit a changeset for review.
    SubmitChangeset,
    /// Post a comment on a changeset review thread.
    CommentInReview,
    /// Approve, request changes, or reject a changeset.
    ReviewChangeset,
    /// Move any user's conflicted/needs_revalidation changeset back to draft.
    MoveToDraftAny,
    /// Move only the caller's own conflicted/needs_revalidation changeset to
    /// draft (User and Reviewer get this, not MoveToDraftAny).
    MoveToDraftOwn,
    /// Assemble a release from the queue.
    AssembleRelease,
    /// Publish a release (tag + update integration branch).
    PublishRelease,
    /// Deploy or promote a release to an environment.
    DeployRelease,
    /// Approve a skip-stage or concurrent multi-env deployment.
    ApproveSkipStage,
    /// Invite new users to the app.
    InviteUsers,
    /// Manage app settings, roles, and environment metadata.
    ManageApp,
}

impl Capability {
    /// Minimum role required to exercise this capability.
    pub fn min_role(&self) -> Role {
        match self {
            Capability::ReadApp
            | Capability::EditOwnWorkspace
            | Capability::EditOwnChangeset
            | Capability::SubmitChangeset
            | Capability::CommentInReview
            | Capability::MoveToDraftOwn => Role::Member,

            Capability::ReviewChangeset
            | Capability::ApproveSkipStage => Role::Reviewer,

            Capability::MoveToDraftAny
            | Capability::AssembleRelease
            | Capability::PublishRelease
            | Capability::DeployRelease => Role::ConfigManager,

            Capability::InviteUsers
            | Capability::ManageApp => Role::Admin,
        }
    }
}

// ---------------------------------------------------------------------------
// User
// ---------------------------------------------------------------------------

/// Stored in the `users` MongoDB collection. Password hash is never sent over
/// the API; the corresponding API response type omits it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    #[serde(rename = "_id")]
    pub id: ObjectId,
    pub email: String,
    /// Argon2id hash. Never exposed outside conman-auth.
    pub password_hash: String,
    pub name: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// App membership
// ---------------------------------------------------------------------------

/// One record per (user, app) pair. Stored in `repo_memberships`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppMembership {
    #[serde(rename = "_id")]
    pub id: ObjectId,
    pub user_id: ObjectId,
    pub repo_id: ObjectId,
    pub role: Role,
    pub created_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// Invite
// ---------------------------------------------------------------------------

/// Pending or accepted invitation. Stored in `invites`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Invite {
    #[serde(rename = "_id")]
    pub id: ObjectId,
    pub repo_id: ObjectId,
    /// Email of the person being invited.
    pub email: String,
    /// Role granted upon acceptance.
    pub role: Role,
    /// Opaque URL-safe token (base64url-encoded 32 random bytes).
    pub token: String,
    /// User ID of the admin who created this invite.
    pub invited_by: ObjectId,
    pub expires_at: DateTime<Utc>,
    /// Set when the invite is accepted; None while pending.
    pub accepted_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// Password reset token
// ---------------------------------------------------------------------------

/// Stored in `password_reset_tokens`. Short-lived, single-use.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PasswordResetToken {
    #[serde(rename = "_id")]
    pub id: ObjectId,
    pub user_id: ObjectId,
    /// Opaque URL-safe token (base64url-encoded 32 random bytes).
    pub token: String,
    pub expires_at: DateTime<Utc>,
    /// Set when the token is consumed; None while unused.
    pub used_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}
```

### Auth types (`conman-auth`)

```rust
use chrono::{DateTime, Utc};
use mongodb::bson::oid::ObjectId;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use conman_core::{Capability, ConmanError, Role};

// ---------------------------------------------------------------------------
// JWT claims
// ---------------------------------------------------------------------------

/// Payload encoded in the JWT. Kept minimal — memberships are loaded from
/// the database on each request so that role changes take effect immediately.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    /// Subject: user ID as hex string.
    pub sub: String,
    /// User email (convenience; not used for authz).
    pub email: String,
    /// Issued-at (seconds since epoch).
    pub iat: i64,
    /// Expiration (seconds since epoch).
    pub exp: i64,
}

// ---------------------------------------------------------------------------
// AuthUser (request-scoped, populated by middleware)
// ---------------------------------------------------------------------------

/// Extracted from the JWT and enriched with live membership data. Stored in
/// Axum request extensions so handlers can access it via
/// `Extension<AuthUser>`.
#[derive(Debug, Clone)]
pub struct AuthUser {
    pub user_id: ObjectId,
    pub email: String,
    /// Mapping of repo_id -> role for every app this user is a member of.
    /// Loaded from `repo_memberships` on every request.
    pub roles: HashMap<ObjectId, Role>,
}

impl AuthUser {
    /// Require the user to have at least `required` role for the given app.
    /// Returns `ConmanError::Forbidden` on failure.
    pub fn require_role(&self, repo_id: &ObjectId, required: Role) -> Result<(), ConmanError> {
        match self.roles.get(repo_id) {
            Some(role) if role.satisfies(required) => Ok(()),
            _ => Err(ConmanError::Forbidden {
                message: format!(
                    "requires role {} on app {}",
                    required,
                    repo_id.to_hex()
                ),
            }),
        }
    }

    /// Require the user to have a capability for the given app. Uses the
    /// capability's `min_role()` to determine the threshold.
    pub fn require_capability(
        &self,
        repo_id: &ObjectId,
        capability: Capability,
    ) -> Result<(), ConmanError> {
        self.require_role(repo_id, capability.min_role())
    }

    /// Returns the user's role for an app, if any.
    pub fn role_for(&self, repo_id: &ObjectId) -> Option<Role> {
        self.roles.get(repo_id).copied()
    }
}

// ---------------------------------------------------------------------------
// Password policy
// ---------------------------------------------------------------------------

/// Minimum password requirements enforced on registration and reset.
pub struct PasswordPolicy;

impl PasswordPolicy {
    pub const MIN_LENGTH: usize = 8;
    pub const MAX_LENGTH: usize = 128;

    /// Validate a plaintext password against the policy. Returns a
    /// human-readable error message on failure.
    pub fn validate(password: &str) -> Result<(), ConmanError> {
        if password.len() < Self::MIN_LENGTH {
            return Err(ConmanError::Validation {
                message: format!(
                    "password must be at least {} characters",
                    Self::MIN_LENGTH
                ),
            });
        }
        if password.len() > Self::MAX_LENGTH {
            return Err(ConmanError::Validation {
                message: format!(
                    "password must be at most {} characters",
                    Self::MAX_LENGTH
                ),
            });
        }
        Ok(())
    }
}
```

### API types (`conman-api`)

```rust
use mongodb::bson::oid::ObjectId;
use serde::{Deserialize, Serialize};

use conman_core::Role;

// ---------------------------------------------------------------------------
// Auth endpoints
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
}

#[derive(Debug, Serialize)]
pub struct LoginResponse {
    pub token: String,
    pub user: UserSummary,
}

#[derive(Debug, Serialize)]
pub struct UserSummary {
    pub id: String,
    pub email: String,
    pub name: String,
}

#[derive(Debug, Deserialize)]
pub struct ForgotPasswordRequest {
    pub email: String,
}

#[derive(Debug, Deserialize)]
pub struct ResetPasswordRequest {
    pub token: String,
    pub new_password: String,
}

// ---------------------------------------------------------------------------
// Invite endpoints
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct InviteRequest {
    pub email: String,
    pub role: Role,
}

#[derive(Debug, Serialize)]
pub struct InviteResponse {
    pub id: String,
    pub repo_id: String,
    pub email: String,
    pub role: Role,
    pub expires_at: String,
    pub created_at: String,
}

#[derive(Debug, Deserialize)]
pub struct AcceptInviteRequest {
    pub token: String,
    pub name: String,
    pub password: String,
}

#[derive(Debug, Serialize)]
pub struct AcceptInviteResponse {
    pub token: String,
    pub user: UserSummary,
}

// ---------------------------------------------------------------------------
// Membership endpoints
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct MemberResponse {
    pub user_id: String,
    pub email: String,
    pub name: String,
    pub role: Role,
    pub joined_at: String,
}

// ---------------------------------------------------------------------------
// Notification preferences
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, Deserialize)]
pub struct NotificationPreferences {
    pub email_enabled: bool,
}
```

## 4. Database

### Collection: `users`

| Field | Type | Description |
|-------|------|-------------|
| `_id` | `ObjectId` | Primary key |
| `email` | `String` | Unique login identifier |
| `password_hash` | `String` | Argon2id hash |
| `name` | `String` | Display name |
| `created_at` | `DateTime` | Account creation timestamp |
| `updated_at` | `DateTime` | Last profile update timestamp |

Indexes:

```javascript
// Unique email for login lookups and duplicate prevention
{ "email": 1 }  // unique: true
```

Example document:

```json
{
  "_id": { "$oid": "665a1b2c3d4e5f6a7b8c9d0e" },
  "email": "alice@example.com",
  "password_hash": "$argon2id$v=19$m=19456,t=2,p=1$...",
  "name": "Alice Chen",
  "created_at": { "$date": "2025-06-01T10:00:00Z" },
  "updated_at": { "$date": "2025-06-01T10:00:00Z" }
}
```

### Collection: `repo_memberships`

| Field | Type | Description |
|-------|------|-------------|
| `_id` | `ObjectId` | Primary key |
| `user_id` | `ObjectId` | References `users._id` |
| `repo_id` | `ObjectId` | References `apps._id` |
| `role` | `String` | One of: `user`, `reviewer`, `config_manager`, `admin` |
| `created_at` | `DateTime` | Membership creation timestamp |

Indexes:

```javascript
// Unique pair: one role per user per app
{ "user_id": 1, "repo_id": 1 }  // unique: true

// List all members of an app (used by GET /api/repos/:repoId/members)
{ "repo_id": 1 }

// Load all memberships for a user (used by auth middleware on every request)
{ "user_id": 1 }
```

Example document:

```json
{
  "_id": { "$oid": "665a1c0000000000000000a1" },
  "user_id": { "$oid": "665a1b2c3d4e5f6a7b8c9d0e" },
  "repo_id": { "$oid": "665a1a0000000000000000b1" },
  "role": "config_manager",
  "created_at": { "$date": "2025-06-01T10:05:00Z" }
}
```

### Collection: `invites`

| Field | Type | Description |
|-------|------|-------------|
| `_id` | `ObjectId` | Primary key |
| `repo_id` | `ObjectId` | References `apps._id` |
| `email` | `String` | Invitee email address |
| `role` | `String` | Role to grant on acceptance |
| `token` | `String` | URL-safe opaque token (32 random bytes, base64url) |
| `invited_by` | `ObjectId` | References `users._id` (the admin) |
| `expires_at` | `DateTime` | Token expiry (created_at + 7 days) |
| `accepted_at` | `DateTime?` | Null until accepted |
| `created_at` | `DateTime` | Invite creation timestamp |

Indexes:

```javascript
// Token lookup for accept-invite endpoint
{ "token": 1 }  // unique: true

// List pending invites for an app
{ "repo_id": 1, "accepted_at": 1 }

// Prevent duplicate pending invites to the same email for the same app
{ "repo_id": 1, "email": 1, "accepted_at": 1 }  // unique: true, partialFilterExpression: { "accepted_at": null }
```

Example document:

```json
{
  "_id": { "$oid": "665a1d0000000000000000c1" },
  "repo_id": { "$oid": "665a1a0000000000000000b1" },
  "email": "bob@example.com",
  "role": "reviewer",
  "token": "a1b2c3d4e5f6g7h8i9j0k1l2m3n4o5p6q7r8s9t0u1v2",
  "invited_by": { "$oid": "665a1b2c3d4e5f6a7b8c9d0e" },
  "expires_at": { "$date": "2025-06-08T10:00:00Z" },
  "accepted_at": null,
  "created_at": { "$date": "2025-06-01T10:00:00Z" }
}
```

### Collection: `password_reset_tokens`

| Field | Type | Description |
|-------|------|-------------|
| `_id` | `ObjectId` | Primary key |
| `user_id` | `ObjectId` | References `users._id` |
| `token` | `String` | URL-safe opaque token (32 random bytes, base64url) |
| `expires_at` | `DateTime` | Token expiry (created_at + 1 hour) |
| `used_at` | `DateTime?` | Null until consumed |
| `created_at` | `DateTime` | Token creation timestamp |

Indexes:

```javascript
// Token lookup for reset-password endpoint
{ "token": 1 }  // unique: true

// TTL index: automatically delete expired+used tokens after 24 hours
// Keeps the collection small without manual cleanup
{ "expires_at": 1 }  // expireAfterSeconds: 86400
```

Example document:

```json
{
  "_id": { "$oid": "665a1e0000000000000000d1" },
  "user_id": { "$oid": "665a1b2c3d4e5f6a7b8c9d0e" },
  "token": "x9y8z7w6v5u4t3s2r1q0p9o8n7m6l5k4j3i2h1g0f9e8",
  "expires_at": { "$date": "2025-06-01T11:00:00Z" },
  "used_at": null,
  "created_at": { "$date": "2025-06-01T10:00:00Z" }
}
```

### Collection: `notification_preferences`

| Field | Type | Description |
|-------|------|-------------|
| `_id` | `ObjectId` | Primary key |
| `user_id` | `ObjectId` | References `users._id` |
| `email_enabled` | `bool` | Master toggle for all email notifications |
| `updated_at` | `DateTime` | Last update timestamp |

Indexes:

```javascript
// One preference document per user
{ "user_id": 1 }  // unique: true
```

Example document:

```json
{
  "_id": { "$oid": "665a1f0000000000000000e1" },
  "user_id": { "$oid": "665a1b2c3d4e5f6a7b8c9d0e" },
  "email_enabled": true,
  "updated_at": { "$date": "2025-06-01T10:00:00Z" }
}
```

## 5. API Endpoints

### `POST /api/auth/login`

Authenticate with email and password, receive a JWT.

| | |
|---|---|
| **Auth** | None (public) |
| **RBAC** | None |

Request:

```json
{
  "email": "alice@example.com",
  "password": "s3cure-password"
}
```

Response `200`:

```json
{
  "data": {
    "token": "eyJhbGciOi...",
    "user": {
      "id": "665a1b2c3d4e5f6a7b8c9d0e",
      "email": "alice@example.com",
      "name": "Alice Chen"
    }
  }
}
```

Errors:

| Status | Code | When |
|--------|------|------|
| 401 | `invalid_credentials` | Email not found OR password mismatch (same error to prevent enumeration) |
| 400 | `validation_error` | Missing or empty email/password |

### `POST /api/auth/logout`

Invalidate the current session. In a stateless JWT scheme this is a no-op on
the server; the client discards the token. Included for API completeness and
to support future token blocklisting.

| | |
|---|---|
| **Auth** | Bearer JWT |
| **RBAC** | None (any authenticated user) |

Request: empty body.

Response `200`:

```json
{
  "data": { "message": "logged out" }
}
```

### `POST /api/auth/forgot-password`

Request a password reset email. Always returns 200 regardless of whether the
email exists to prevent enumeration.

| | |
|---|---|
| **Auth** | None (public) |
| **RBAC** | None |

Request:

```json
{
  "email": "alice@example.com"
}
```

Response `200`:

```json
{
  "data": { "message": "if an account exists, a reset email has been sent" }
}
```

Errors:

| Status | Code | When |
|--------|------|------|
| 400 | `validation_error` | Missing or malformed email |

### `POST /api/auth/reset-password`

Consume a reset token and set a new password.

| | |
|---|---|
| **Auth** | None (public) |
| **RBAC** | None |

Request:

```json
{
  "token": "x9y8z7w6v5u4t3s2r1q0...",
  "new_password": "n3w-s3cure-password"
}
```

Response `200`:

```json
{
  "data": { "message": "password updated" }
}
```

Errors:

| Status | Code | When |
|--------|------|------|
| 410 | `token_expired` | Token past `expires_at` |
| 400 | `token_invalid` | Token not found or already used |
| 400 | `validation_error` | Password does not meet policy |

### `POST /api/auth/accept-invite`

Accept an invite token, create a user account (if new), create the app
membership, and return a JWT so the user is immediately logged in.

| | |
|---|---|
| **Auth** | None (public) |
| **RBAC** | None |

Request:

```json
{
  "token": "a1b2c3d4e5f6g7h8...",
  "name": "Bob Smith",
  "password": "b0b-s3cure-password"
}
```

Response `200`:

```json
{
  "data": {
    "token": "eyJhbGciOi...",
    "user": {
      "id": "665a1b2c000000000000f001",
      "email": "bob@example.com",
      "name": "Bob Smith"
    }
  }
}
```

Errors:

| Status | Code | When |
|--------|------|------|
| 410 | `invite_expired` | Invite past `expires_at` |
| 400 | `invite_invalid` | Token not found or already accepted |
| 400 | `validation_error` | Password does not meet policy |
| 409 | `conflict` | User already has a membership for this app |

### `GET /api/repos/:repoId/members?page=&limit=`

List members of an app with their roles. Paginated.

| | |
|---|---|
| **Auth** | Bearer JWT |
| **RBAC** | `ReadApp` (any member of the app) |

Response `200`:

```json
{
  "data": [
    {
      "user_id": "665a1b2c3d4e5f6a7b8c9d0e",
      "email": "alice@example.com",
      "name": "Alice Chen",
      "role": "admin",
      "joined_at": "2025-06-01T10:05:00Z"
    }
  ],
  "pagination": { "page": 1, "limit": 20, "total": 3 }
}
```

Errors:

| Status | Code | When |
|--------|------|------|
| 404 | `not_found` | App does not exist |
| 403 | `forbidden` | User is not a member of this app |

### `POST /api/teams/:teamId/invites`

Create an invite for a new user. Only app admins can invite.

| | |
|---|---|
| **Auth** | Bearer JWT |
| **RBAC** | `InviteUsers` (`admin` only) |

Request:

```json
{
  "email": "carol@example.com",
  "role": "reviewer"
}
```

Response `201`:

```json
{
  "data": {
    "id": "665a1d0000000000000000c2",
    "repo_id": "665a1a0000000000000000b1",
    "email": "carol@example.com",
    "role": "reviewer",
    "expires_at": "2025-06-08T10:00:00Z",
    "created_at": "2025-06-01T10:00:00Z"
  }
}
```

Errors:

| Status | Code | When |
|--------|------|------|
| 403 | `forbidden` | Caller is not admin |
| 409 | `conflict` | Pending invite already exists for this email+app |
| 409 | `conflict` | User is already a member of this app |
| 404 | `not_found` | App does not exist |

### `POST /api/teams/:teamId/invites/:inviteId/resend`

Resend the invite email. Resets `expires_at` to 7 days from now.

| | |
|---|---|
| **Auth** | Bearer JWT |
| **RBAC** | `InviteUsers` (`admin` only) |

Request: empty body.

Response `200`:

```json
{
  "data": {
    "id": "665a1d0000000000000000c2",
    "repo_id": "665a1a0000000000000000b1",
    "email": "carol@example.com",
    "role": "reviewer",
    "expires_at": "2025-06-08T12:30:00Z",
    "created_at": "2025-06-01T10:00:00Z"
  }
}
```

Errors:

| Status | Code | When |
|--------|------|------|
| 403 | `forbidden` | Caller is not admin |
| 404 | `not_found` | Invite does not exist |
| 400 | `invite_invalid` | Invite already accepted |

### `DELETE /api/teams/:teamId/invites/:inviteId`

Revoke a pending invite.

| | |
|---|---|
| **Auth** | Bearer JWT |
| **RBAC** | `InviteUsers` (`admin` only) |

Request: empty body.

Response `200`:

```json
{
  "data": { "message": "invite revoked" }
}
```

Errors:

| Status | Code | When |
|--------|------|------|
| 403 | `forbidden` | Caller is not admin |
| 404 | `not_found` | Invite does not exist |
| 400 | `invite_invalid` | Invite already accepted (cannot revoke) |

### `GET /api/me/notification-preferences`

Get the current user's notification preferences.

| | |
|---|---|
| **Auth** | Bearer JWT |
| **RBAC** | None (any authenticated user) |

Response `200`:

```json
{
  "data": {
    "email_enabled": true
  }
}
```

### `PATCH /api/me/notification-preferences`

Update the current user's notification preferences.

| | |
|---|---|
| **Auth** | Bearer JWT |
| **RBAC** | None (any authenticated user) |

Request:

```json
{
  "email_enabled": false
}
```

Response `200`:

```json
{
  "data": {
    "email_enabled": false
  }
}
```

## 6. Business Logic

### Password hashing (argon2)

Use `argon2` crate with Argon2id variant and recommended OWASP parameters:

```rust
use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2, Algorithm, Params, Version,
};

/// Build the Argon2id hasher with production cost parameters.
/// OWASP 2024 recommendation: m=19456 KiB (19 MiB), t=2, p=1.
fn build_argon2() -> Argon2<'static> {
    let params = Params::new(19456, 2, 1, None)
        .expect("valid argon2 params");
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
}

/// Hash a plaintext password. Returns the PHC-format hash string.
pub fn hash_password(password: &str) -> Result<String, ConmanError> {
    let salt = SaltString::generate(&mut OsRng);
    let argon2 = build_argon2();
    let hash = argon2
        .hash_password(password.as_bytes(), &salt)
        .map_err(|e| ConmanError::Internal {
            message: format!("password hashing failed: {e}"),
        })?;
    Ok(hash.to_string())
}

/// Verify a plaintext password against a stored PHC-format hash.
pub fn verify_password(password: &str, hash: &str) -> Result<bool, ConmanError> {
    let parsed_hash = PasswordHash::new(hash).map_err(|e| ConmanError::Internal {
        message: format!("invalid stored password hash: {e}"),
    })?;
    let argon2 = build_argon2();
    Ok(argon2.verify_password(password.as_bytes(), &parsed_hash).is_ok())
}
```

### JWT issuance and validation

```rust
use chrono::{Duration, Utc};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};

pub struct JwtConfig {
    pub secret: String,
    pub expiry_hours: i64,
}

/// Issue a JWT for a successfully authenticated user.
pub fn issue_token(
    user_id: &ObjectId,
    email: &str,
    config: &JwtConfig,
) -> Result<String, ConmanError> {
    let now = Utc::now();
    let claims = Claims {
        sub: user_id.to_hex(),
        email: email.to_string(),
        iat: now.timestamp(),
        exp: (now + Duration::hours(config.expiry_hours)).timestamp(),
    };
    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(config.secret.as_bytes()),
    )
    .map_err(|e| ConmanError::Internal {
        message: format!("JWT encoding failed: {e}"),
    })
}

/// Decode and validate a JWT. Returns the claims on success.
pub fn validate_token(token: &str, config: &JwtConfig) -> Result<Claims, ConmanError> {
    let token_data = decode::<Claims>(
        token,
        &DecodingKey::from_secret(config.secret.as_bytes()),
        &Validation::default(),
    )
    .map_err(|e| ConmanError::Forbidden {
        message: format!("invalid token: {e}"),
    })?;
    Ok(token_data.claims)
}
```

### Invite flow

```
1. admin calls POST /api/teams/:teamId/invites { email, role }
2. Server validates:
   - Caller has InviteUsers capability (admin role)
   - No pending invite exists for this email+app
   - Email is not already a member of this app
3. Server generates a 32-byte random token (base64url-encoded)
4. Server creates Invite document with expires_at = now + 7 days
5. Server sends invite email with link containing the token
6. Server emits audit event (action: "invite_created")

Accept flow:
1. Invitee calls POST /api/auth/accept-invite { token, name, password }
2. Server looks up invite by token
3. Server validates:
   - Invite exists and accepted_at is null
   - Invite has not expired (expires_at > now)
   - Password meets policy
4. Server creates User (if email not already registered) with hashed password
5. Server creates AppMembership { user_id, repo_id, role }
6. Server sets invite.accepted_at = now
7. Server issues JWT and returns it so the user is immediately logged in
8. Server emits audit event (action: "invite_accepted")
```

### Password reset flow

```
1. User calls POST /api/auth/forgot-password { email }
2. Server always returns 200 (no email enumeration)
3. If email exists in users collection:
   a. Generate 32-byte random token (base64url-encoded)
   b. Create PasswordResetToken with expires_at = now + 1 hour
   c. Send reset email with link containing the token
4. If email does not exist: do nothing, still return 200

Reset flow:
1. User calls POST /api/auth/reset-password { token, new_password }
2. Server looks up token in password_reset_tokens
3. Server validates:
   - Token exists and used_at is null
   - Token has not expired (expires_at > now) -> 410 if expired
   - New password meets policy
4. Server hashes new password and updates users.password_hash
5. Server sets token.used_at = now
6. Server returns success
```

### RBAC check logic

Role inheritance is enforced by `PartialOrd` on the `Role` enum. The
discriminant ordering (`User=0 < Reviewer=1 < ConfigManager=2 < AppAdmin=3`)
means `admin.satisfies(config_manager)` is true, implementing the
inheritance rule from the scope doc.

```
Per-request flow:
1. Handler calls auth_user.require_capability(repo_id, Capability::X)
2. require_capability looks up capability.min_role()
3. require_capability calls require_role(repo_id, min_role)
4. require_role looks up the user's role for that app in the roles HashMap
5. If role >= min_role -> Ok(())
6. If role < min_role or no membership -> Err(ConmanError::Forbidden)
```

Special case: `MoveToDraftOwn` vs `MoveToDraftAny`. The handler must
additionally check ownership when the user's role is below ConfigManager:

```rust
// In the move-to-draft handler:
let user_role = auth_user.role_for(&repo_id);
let is_owner = changeset.author_user_id == auth_user.user_id;

match (user_role, is_owner) {
    // ConfigManager and above can move any changeset to draft
    (Some(role), _) if role.satisfies(Role::ConfigManager) => Ok(()),
    // User and Reviewer can only move their own
    (Some(_), true) => Ok(()),
    _ => Err(ConmanError::Forbidden {
        message: "only the author or a config_manager+ can move to draft".into(),
    }),
}
```

### Auth middleware

```rust
use axum::{
    extract::{Request, State},
    middleware::Next,
    response::Response,
};

/// Axum middleware that extracts and validates the JWT from the Authorization
/// header, loads the user's app memberships, and populates AuthUser in
/// request extensions.
pub async fn auth_middleware(
    State(state): State<AppState>,
    mut request: Request,
    next: Next,
) -> Result<Response, ConmanError> {
    // Extract the Bearer token from the Authorization header
    let token = request
        .headers()
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .ok_or_else(|| ConmanError::Forbidden {
            message: "missing or malformed authorization header".into(),
        })?;

    // Validate the JWT and extract claims
    let claims = validate_token(token, &state.jwt_config)?;

    // Parse the user ID from the subject claim
    let user_id = ObjectId::parse_str(&claims.sub).map_err(|_| ConmanError::Forbidden {
        message: "invalid user ID in token".into(),
    })?;

    // Load all app memberships for this user from MongoDB
    let memberships = state
        .membership_repo
        .find_by_user_id(&user_id)
        .await
        .map_err(|e| ConmanError::Internal {
            message: format!("failed to load memberships: {e}"),
        })?;

    // Build the roles map: repo_id -> role
    let roles: HashMap<ObjectId, Role> = memberships
        .into_iter()
        .map(|m| (m.repo_id, m.role))
        .collect();

    // Populate the AuthUser in request extensions
    let auth_user = AuthUser {
        user_id,
        email: claims.email,
        roles,
    };
    request.extensions_mut().insert(auth_user);

    Ok(next.run(request).await)
}
```

## 7. Gitaly-rs Integration

N/A for this epic.

## 8. Implementation Checklist

Ordered TDD steps. Each step is a unit of work that can be implemented and
tested independently.

- [ ] **E02-S01**: Define `Role` enum with `PartialOrd` and `Serialize`/`Deserialize` in `conman-core`. Write unit tests for `satisfies()` ordering (`User < Reviewer < ConfigManager < AppAdmin`).
- [ ] **E02-S02**: Define `Capability` enum and `min_role()` mapping in `conman-core`. Write unit tests verifying each capability maps to the correct minimum role per the permission matrix.
- [ ] **E02-S03**: Define `User`, `AppMembership`, `Invite`, `PasswordResetToken` domain structs in `conman-core`.
- [ ] **E02-S04**: Implement `PasswordPolicy::validate()` in `conman-auth`. Write unit tests for min/max length enforcement.
- [ ] **E02-S05**: Implement `hash_password()` and `verify_password()` in `conman-auth` with Argon2id. Write unit tests: hash then verify succeeds, verify with wrong password fails, verify with corrupted hash returns error.
- [ ] **E02-S06**: Implement `issue_token()` and `validate_token()` in `conman-auth`. Write unit tests: roundtrip encode/decode, expired token rejected, tampered token rejected.
- [ ] **E02-S07**: Define `Claims` and `AuthUser` structs in `conman-auth`. Write unit tests for `require_role()`, `require_capability()`, and `role_for()` methods.
- [ ] **E02-S08**: Create `UserRepo` in `conman-db` with `ensure_indexes()`, `insert()`, `find_by_id()`, `find_by_email()`, `update_password()`. Integration test with MongoDB.
- [ ] **E02-S09**: Create `MembershipRepo` in `conman-db` with `ensure_indexes()`, `insert()`, `find_by_user_id()`, `find_by_app_id()` (paginated), `find_by_user_and_app()`, `delete()`. Integration test.
- [ ] **E02-S10**: Create `InviteRepo` in `conman-db` with `ensure_indexes()`, `insert()`, `find_by_token()`, `find_by_app_id()` (paginated, filter pending), `find_pending_by_email_and_app()`, `mark_accepted()`, `update_expires_at()`, `delete()`. Integration test.
- [ ] **E02-S11**: Create `PasswordResetTokenRepo` in `conman-db` with `ensure_indexes()`, `insert()`, `find_by_token()`, `mark_used()`. Integration test.
- [ ] **E02-S12**: Create `NotificationPreferencesRepo` in `conman-db` with `ensure_indexes()`, `upsert()`, `find_by_user_id()`. Integration test.
- [ ] **E02-S13**: Implement auth middleware in `conman-api`. Unit test with mock repo: valid token populates `AuthUser`, missing header returns 403, expired token returns 403.
- [ ] **E02-S14**: Implement `POST /api/auth/login` handler. Integration test: valid credentials return JWT, invalid password returns 401, nonexistent email returns 401 (same error).
- [ ] **E02-S15**: Implement `POST /api/auth/logout` handler. Integration test: returns 200.
- [ ] **E02-S16**: Implement `POST /api/auth/forgot-password` handler. Integration test: existing email creates token, nonexistent email still returns 200.
- [ ] **E02-S17**: Implement `POST /api/auth/reset-password` handler. Integration test: valid token updates password, expired token returns 410, used token returns 400.
- [ ] **E02-S18**: Implement `POST /api/auth/accept-invite` handler. Integration test: valid invite creates user + membership + returns JWT, expired invite returns 410, already-accepted returns 400.
- [ ] **E02-S19**: Implement `GET /api/repos/:repoId/members` handler with pagination. Integration test: returns members with roles, respects pagination, non-member returns 403.
- [ ] **E02-S20**: Implement `POST /api/teams/:teamId/invites` handler. Integration test: admin can invite, non-admin returns 403, duplicate invite returns 409.
- [ ] **E02-S21**: Implement `POST /api/teams/:teamId/invites/:inviteId/resend` handler. Integration test: resets expiry, already-accepted returns 400.
- [ ] **E02-S22**: Implement `DELETE /api/teams/:teamId/invites/:inviteId` handler. Integration test: deletes pending invite, already-accepted returns 400.
- [ ] **E02-S23**: Implement `GET /api/me/notification-preferences` handler. Integration test: returns defaults for new user, returns saved preferences.
- [ ] **E02-S24**: Implement `PATCH /api/me/notification-preferences` handler. Integration test: updates toggle, subsequent GET reflects change.
- [ ] **E02-S25**: Add audit event emission to all mutation handlers (invite_created, invite_accepted, invite_revoked, invite_resent, password_reset_requested, password_changed, membership_created, notification_preferences_updated). Verify audit documents in integration tests.
- [ ] **E02-S26**: End-to-end RBAC integration test: create app with admin, invite reviewer and user, verify each role can/cannot perform operations matching the full permission matrix.

## 9. Test Cases

### Authentication

| # | Test | Expected |
|---|------|----------|
| 1 | Login with valid email and password | 200, body contains JWT and user summary |
| 2 | Login with valid email and wrong password | 401, `invalid_credentials` (no hint about which field) |
| 3 | Login with nonexistent email | 401, `invalid_credentials` (same error as wrong password) |
| 4 | Login with empty email | 400, `validation_error` |
| 5 | Login with empty password | 400, `validation_error` |

### JWT

| # | Test | Expected |
|---|------|----------|
| 6 | Auth middleware with valid Bearer token | `AuthUser` populated in request extensions |
| 7 | Auth middleware with expired token | 403, `forbidden` |
| 8 | Auth middleware with missing Authorization header | 403, `forbidden` |
| 9 | Auth middleware with malformed token (not valid JWT) | 403, `forbidden` |
| 10 | Auth middleware with tampered payload (invalid signature) | 403, `forbidden` |

### RBAC

| # | Test | Expected |
|---|------|----------|
| 11 | User role cannot approve changeset (`ReviewChangeset`) | 403, `forbidden` |
| 12 | Reviewer role can approve changeset (`ReviewChangeset`) | allowed |
| 13 | ConfigManager role can approve changeset (`ReviewChangeset`) | allowed (inherits) |
| 14 | AppAdmin role inherits all ConfigManager capabilities | allowed for `AssembleRelease`, `PublishRelease`, `DeployRelease` |
| 15 | User can move own changeset to draft (`MoveToDraftOwn`) | allowed |
| 16 | User cannot move another user's changeset to draft | 403, `forbidden` |
| 17 | ConfigManager can move any changeset to draft (`MoveToDraftAny`) | allowed |
| 18 | User cannot invite users (`InviteUsers`) | 403, `forbidden` |
| 19 | Reviewer cannot invite users (`InviteUsers`) | 403, `forbidden` |
| 20 | AppAdmin can invite users (`InviteUsers`) | allowed |
| 21 | Non-member of app cannot access app resources | 403, `forbidden` |

### Invites

| # | Test | Expected |
|---|------|----------|
| 22 | AppAdmin creates invite | 201, invite document persisted |
| 23 | Non-admin creates invite | 403, `forbidden` |
| 24 | Duplicate pending invite for same email+app | 409, `conflict` |
| 25 | Invite for email that is already a member | 409, `conflict` |
| 26 | Accept invite with valid token | 200, user created, membership created, JWT returned |
| 27 | Accept invite for existing user (already registered for another app) | 200, uses existing user, creates new membership |
| 28 | Accept invite with expired token | 410, `invite_expired` |
| 29 | Accept invite with already-accepted token | 400, `invite_invalid` |
| 30 | Accept invite with nonexistent token | 400, `invite_invalid` |
| 31 | Accept invite with password too short | 400, `validation_error` |
| 32 | Resend invite resets expiry to 7 days from now | 200, `expires_at` updated |
| 33 | Resend already-accepted invite | 400, `invite_invalid` |
| 34 | Delete pending invite | 200, invite removed |
| 35 | Delete already-accepted invite | 400, `invite_invalid` |

### Password reset

| # | Test | Expected |
|---|------|----------|
| 36 | Forgot password for existing email | 200, reset token created (verify in DB) |
| 37 | Forgot password for nonexistent email | 200, no token created (no error either) |
| 38 | Reset password with valid token | 200, password updated, old password no longer works, new password works |
| 39 | Reset password with expired token | 410, `token_expired` |
| 40 | Reset password with already-used token | 400, `token_invalid` |
| 41 | Reset password with nonexistent token | 400, `token_invalid` |
| 42 | Reset password with password too short | 400, `validation_error` |

### Members

| # | Test | Expected |
|---|------|----------|
| 43 | List members returns all users with roles | 200, paginated list |
| 44 | List members respects page and limit | correct subset returned |
| 45 | List members by non-member of app | 403, `forbidden` |

### Notification preferences

| # | Test | Expected |
|---|------|----------|
| 46 | GET preferences for new user returns default (email_enabled: true) | 200, default document |
| 47 | PATCH preferences updates toggle | 200, updated value |
| 48 | GET after PATCH reflects the change | 200, matches patched value |

### Password policy

| # | Test | Expected |
|---|------|----------|
| 49 | Password with 7 characters rejected | `validation_error` |
| 50 | Password with 8 characters accepted | Ok |
| 51 | Password with 128 characters accepted | Ok |
| 52 | Password with 129 characters rejected | `validation_error` |

### Password hashing

| # | Test | Expected |
|---|------|----------|
| 53 | Hash then verify with correct password | true |
| 54 | Hash then verify with wrong password | false |
| 55 | Two hashes of the same password are different (unique salts) | hashes differ |

## 10. Acceptance Criteria

- [ ] All API endpoints listed in section 5 are implemented and return the documented response shapes.
- [ ] Unauthenticated requests to any endpoint except `/api/auth/login`, `/api/auth/forgot-password`, `/api/auth/reset-password`, and `/api/auth/accept-invite` return 403.
- [ ] Login returns identical error responses for "email not found" and "wrong password" (no email enumeration).
- [ ] Forgot-password always returns 200 regardless of whether the email exists (no email enumeration).
- [ ] Passwords are stored as Argon2id hashes; plaintext passwords never appear in logs, database documents, or API responses.
- [ ] JWT tokens expire after the configured duration (`CONMAN_JWT_EXPIRY_HOURS`, default 24h).
- [ ] Invite tokens expire after 7 days (`CONMAN_INVITE_EXPIRY_DAYS`). Resend resets the expiry.
- [ ] Password reset tokens expire after 1 hour and are single-use.
- [ ] The full RBAC permission matrix from scope doc section 4.1 is enforced:
  - `user`: read, edit own workspace/changeset, submit, comment, move own to draft.
  - `reviewer`: all of `user` plus approve/request changes/reject, skip-stage approval.
  - `config_manager`: all of `reviewer` plus assemble/publish/deploy release, move any to draft.
  - `admin`: all of `config_manager` plus invite users, manage app settings.
- [ ] Role inheritance works: `admin` satisfies any `config_manager` check, `config_manager` satisfies any `reviewer` check, etc.
- [ ] All mutation endpoints emit audit events with actor, timestamp, before/after state, and request context.
- [ ] All 55 test cases from section 9 pass.
- [ ] TTL index on `password_reset_tokens.expires_at` automatically cleans up expired tokens.
- [ ] Unique indexes prevent duplicate emails, duplicate memberships, and duplicate pending invites.

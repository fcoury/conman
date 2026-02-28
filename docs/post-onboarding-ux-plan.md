# Post-Onboarding Product Realignment Plan

## 1. Objective

Realign Conman's authenticated experience (everything after signup/onboarding)
with the core product promise:

- users edit repository-backed YAML/config in a workspace,
- users preview safely on temporary URLs,
- users submit changesets for review,
- reviewers evaluate code + semantic impact,
- config managers assemble releases and promote through environments,
- owners/admins manage governance.

This plan intentionally keeps signup/onboarding unchanged.

## 2. Product Principles

1. Task-first, not endpoint-first.
2. IDs and raw JSON are advanced tools, not default UX.
3. Every role sees a clear "next action" on each page.
4. Preview URLs are first-class artifacts in author and reviewer flows.
5. Release/deploy are pipeline experiences with explicit promotion state.

## 3. Current Gaps

1. Post-onboarding pages still resemble an API control panel.
2. Core actions require manual IDs and free-form payload entry.
3. The review flow does not foreground semantic impact and decision context.
4. Release/deploy flow lacks strong pipeline framing and environment clarity.
5. Temporary environments are not integrated enough into daily author flow.

## 4. Target Information Architecture

Primary sections after onboarding:

1. Build
- Draft Changes
- Changesets

2. Review
- Review Queue (role-aware subset of changesets)

3. Release
- Releases
- Deployments

4. Operate
- Runtime
- Jobs
- Temp Environments

5. Admin
- Apps
- Members
- Notifications
- Settings

## 5. Role-Centered Experience Targets

## 5.1 Member (Author)

1. Create/select workspace.
2. Edit YAML/config with immediate validation.
3. Save to workspace branch with clear commit summary.
4. Create preview URL from workspace.
5. Open changeset draft directly from workspace context.

## 5.2 Reviewer

1. Open review queue filtered to actionable changesets.
2. Inspect semantic and raw diff.
3. Comment inline/thread style (API-compatible)
4. Approve / request changes / reject with rationale.

## 5.3 Config Manager

1. Manage queue composition for release batch.
2. Compare impact before publish.
3. Promote by environment stage with explicit gates.
4. Track deploy job outcomes and drift indicators.

## 5.4 Admin/Owner

1. Manage app surfaces/domains and access.
2. Manage members/invites and role policy.
3. Manage instance-level settings and observability.

## 6. Execution Phases

## Phase 1: Author + Reviewer Foundation

1. Draft Changes redesign
- structured workspace control panel,
- integrated YAML editing workflow,
- quick changeset creation from selected workspace,
- clearer save/reset/checkpoint affordances.

2. Changesets redesign
- list/detail split,
- status filters,
- context-sensitive primary actions,
- review-first information hierarchy.

## Phase 2: Release + Deploy Pipeline

1. Releases page becomes release composer.
2. Deployments page becomes environment pipeline/promotion view.
3. Remove manual CSV/JSON-first actions from default path.

## Phase 3: Operations + Admin Simplification

1. Runtime page typed forms first, advanced JSON second.
2. Temp environments presented as preview assets with TTL controls.
3. Members/settings UX cleanup around governance workflows.

## Phase 4: Quality + Validation

1. Accessibility pass (labels, keyboard, aria-live).
2. Playwright regression + role-based scenario matrix.
3. Performance cleanup for heavy lists and polling views.

## 7. UX/Implementation Standards

1. Default state uses guided controls (selects, buttons, clear forms).
2. Advanced payload editors are behind explicit disclosure.
3. Every destructive action requires clear affordance and status feedback.
4. Async actions surface progress and completion clearly.
5. Keep visual language consistent with existing design system tokens.

## 8. Acceptance Criteria

1. A new member can edit config and submit a reviewable changeset without
manual IDs or raw JSON input.
2. A reviewer can complete approve/request-change/reject with semantic diff
visible in one page.
3. A config manager can assemble and publish a release using workflow UI,
not payload editing.
4. A config manager can deploy/promote with clear environment context.
5. Advanced payload tools remain available but optional.

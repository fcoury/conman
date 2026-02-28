# Post-Onboarding UX Backlog

Legend:

- Priority: P0 (must), P1 (next), P2 (later)
- Status: todo, in_progress, done

## Epic E1: Author Workflow (Draft Changes)

### E1-1 Draft Changes information hierarchy
- Priority: P0
- Status: in_progress
- Scope:
  - Replace API-console framing with a 3-step author flow.
  - Add clear workspace summary and focused editor context.
- Acceptance:
  - User can understand "where they are" and "what to do next" from page
    content alone.

### E1-2 Workspace actions and safety
- Priority: P0
- Status: in_progress
- Scope:
  - Make sync/reset/checkpoint actions explicit and visible.
  - Improve save/delete error and validation messaging.
- Acceptance:
  - Validation errors are shown inline before write.
  - Failed actions are surfaced consistently.

### E1-3 Create changeset from workspace
- Priority: P0
- Status: in_progress
- Scope:
  - Add inline "create changeset" action from selected workspace.
  - Pre-fill title/description and avoid manual navigation friction.
- Acceptance:
  - User can create a changeset from Draft Changes in one action flow.

## Epic E2: Reviewer Workflow (Changesets)

### E2-1 Changeset list/detail split
- Priority: P0
- Status: in_progress
- Scope:
  - Left list with status pills + counts.
  - Right detail with selected changeset context.
- Acceptance:
  - Selecting a changeset updates all detail actions without manual ID input.

### E2-2 Review actions and semantic-first model
- Priority: P0
- Status: in_progress
- Scope:
  - Keep semantic diff as default mode.
  - Present approve/request changes/reject as primary reviewer actions.
- Acceptance:
  - Reviewer can complete review without touching advanced controls.

### E2-3 Queue transition clarity
- Priority: P1
- Status: todo
- Scope:
  - Highlight when changeset is queue-eligible.
  - Show queue action states and completion feedback.

## Epic E3: Release Composer UX

### E3-1 Release draft and composition model
- Priority: P1
- Status: todo
- Scope:
  - Replace free-form IDs with selectable queued changesets.
  - Clarify sequence: create -> compose -> reorder -> assemble -> publish.

### E3-2 Impact visibility
- Priority: P1
- Status: todo
- Scope:
  - Show selected changesets and high-level impact summary before publish.

## Epic E4: Deployment Pipeline UX

### E4-1 Environment pipeline framing
- Priority: P1
- Status: todo
- Scope:
  - Visualize environment chain and selected release context.
  - Remove CSV-centric approvals from default path.

### E4-2 Deployment history usability
- Priority: P1
- Status: todo
- Scope:
  - Improve scanning and filtering of deployment history.
  - Show outcome/state prominently.

## Epic E5: Operations and Governance UX

### E5-1 Runtime typed forms first
- Priority: P1
- Status: todo
- Scope:
  - Move JSON payload editing behind advanced disclosure.

### E5-2 Temp envs as preview assets
- Priority: P1
- Status: todo
- Scope:
  - Emphasize preview URL, TTL, source linkage.

### E5-3 Members/admin cleanup
- Priority: P2
- Status: todo
- Scope:
  - Reduce internal-ID reliance and streamline admin forms.

## Epic E6: Quality

### E6-1 Accessibility pass
- Priority: P0
- Status: todo
- Scope:
  - Ensure labels, keyboard interaction, aria-live status updates.

### E6-2 Regression coverage
- Priority: P0
- Status: todo
- Scope:
  - Add/update tests for redesigned Draft Changes and Changesets pages.

## Current Sprint (Now)

1. E1-1, E1-2, E1-3
2. E2-1, E2-2
3. Update progress document and commit

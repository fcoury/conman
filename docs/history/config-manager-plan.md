# DxFlow Config Manager — UI & Feature Plan

## Context

DxFlow is a configuration-driven platform where entire applications (LIMS, patient portals, etc.) are defined in YAML files: entities, pages, queues, providers, workflows, tenant config, and seed data. The existing **DxFlow Studio** is an AI-powered scaffolding tool that generates these configs from natural language.

This plan adds **configuration lifecycle management**: the ability to create working copies of a config, make changes, review diffs (raw + semantic + AI-analyzed), approve via configurable workflows, and promote through configurable environments with auto-deployment. Think "GitHub PRs for DxFlow configs" with domain-aware semantic diffs.

**This is a new standalone application** sharing the Gistia Design System and Studio's look/feel, but independently routed and deployed.

---

## Route Structure

```
/                                          Dashboard: application cards
/apps/:appId                               Application overview (env pipeline + recent changesets)
/apps/:appId/changesets                    Changeset list (filterable by status)
/apps/:appId/changesets/new                Create changeset (modal overlay)
/apps/:appId/changesets/:csId              Changeset workspace (editor + diff + AI + chat)
/apps/:appId/changesets/:csId/review       Review interface (diff + AI analysis + comments + actions)
/apps/:appId/environments                  Environment pipeline + deployment history
/apps/:appId/environments/:envId           Environment detail (deployed version, logs)
/apps/:appId/settings                      Approval stages, environment stages, app metadata
```

---

## Layout Architecture

### Dashboard (`/`)
Full-width, no sidebar. Grid of application cards (name, env count, open changesets). "Add Application" card.

### Application Shell (`/apps/:appId/*`)
Collapsible sidebar + breadcrumb header + content outlet. Reuses Studio's `SidebarProvider` / `SidebarInset` pattern.

**Sidebar contents:**
- Header: Logo + app name
- Nav: Overview, Changesets, Environments, Settings
- Recent Changesets: last 5 with status badges
- Footer: Theme toggle

### Changeset Workspace (`/apps/:appId/changesets/:csId`)
Three resizable panels (same `ResizablePanelGroup` pattern as Studio's `Project.tsx`):

```
┌──file-tree──┬────────editor/diff────────┬──────ai-chat──────┐
│ config/     │ [Edit] [Diff] [AI Summary]│ AI Assistant       │
│  entities/  │                           │                    │
│  * patient  │ type: entity              │ "Add a phone field │
│    .yml [M] │ name: patient             │  to patient"       │
│  pages/     │ fields:                   │                    │
│  + new.yml  │   id:                     │ [AI response...]   │
│    [A]      │   first_name: string      │                    │
│  queues/    │ + phone: string           │ [prompt input]     │
└─────────────┴───────────────────────────┴────────────────────┘
```

- **Left (~20%):** File tree with change markers — `[M]` modified (yellow), `[A]` added (green), `[D]` deleted (red)
- **Center (~50%):** Tabbed area — Edit (YAML editor), Diff (raw/semantic toggle), AI Summary
- **Right (~30%):** Collapsible AI chat panel using `useChat` from AI SDK

---

## Key Screens

### 1. Application Overview

```
┌─ Environment Pipeline ───────────────────────────────────────┐
│  ┌──────┐    ┌──────┐    ┌──────┐    ┌──────┐               │
│  │ Dev  │───>│  QA  │───>│ UAT  │───>│ Prod │               │
│  │ v2.3 │    │ v2.2 │    │ v2.1 │    │ v2.0 │               │
│  └──────┘    └──────┘    └──────┘    └──────┘               │
└──────────────────────────────────────────────────────────────┘

  Recent Changesets                            [+ New Changeset]
  ┌────────────────────────────────────────────────────────────┐
  │ CS-42  Add phone to patient       Draft         Today      │
  │ CS-41  Update order queue         In Review     Yesterday  │
  │ CS-40  Fix provider config        Merged        3 days ago │
  └────────────────────────────────────────────────────────────┘
```

### 2. Changeset List

Filterable tabs: **Open** (draft + submitted + in_review + approved) | **Merged** | **All**

Each card shows: number, title, status badge, author, date, file count, line diff summary (`+18 -2`), AI warning count if any, and primary action button (Edit/Review/Merge).

### 3. Diff Views (toggle between two modes)

**Raw YAML Diff** — side-by-side or unified, syntax highlighted, line numbers, green/red backgrounds for additions/removals. Reviewers can click line numbers to leave inline comments.

**Semantic Diff** — grouped by config type (Entities, Pages, Queues, Providers, Workflows, Tenant), each change described in domain language:

```
ENTITIES                                                    1 change
┌──────────────────────────────────────────────────────────────────┐
│ + Entity `patient`: Added field `email`                          │
│   Type: string  |  Required: true                                │
│   config/entities/patient.yml:13-15                              │
└──────────────────────────────────────────────────────────────────┘

PAGES                                                       2 changes
┌──────────────────────────────────────────────────────────────────┐
│ ~ Page `admin-users`: Added `email` to User Details card         │
│ + Page `new-report`: New page (6 fields, roles: lab_director)    │
└──────────────────────────────────────────────────────────────────┘
```

Symbols: `+` green (addition), `~` amber (modification), `-` red (removal). Each item is clickable → jumps to raw diff location.

### 4. AI Analysis Panel

Three sections:

- **Summary** — 2-3 sentence plain-English overview
- **Impact Analysis** — table of affected areas with severity badges (`info` / `warning` / `critical`)
- **Suggestions** — actionable items with severity: `!` (potential error), `?` (needs verification), `i` (improvement idea)

Generated via `POST /api/apps/:appId/changesets/:csId/analyze` using the same OpenRouter + AI SDK infrastructure as Studio, with a DxFlow-domain-aware system prompt.

### 5. Review Interface

```
┌─ CS-42: Add email field to patient ─────────────────────────┐
│ Author: fcoury  |  Status: In Review  |  Stage 1 of 2       │
│ [Overview]  [Files (3)]  [AI Analysis]  [Comments (2)]       │
├──────────────────────────────────────────────────────────────┤
│ Overview: description + semantic change summary + AI excerpt  │
│ Files: expandable per-file diffs with inline commenting       │
│ AI Analysis: full summary + impact + suggestions              │
│ Comments: threaded discussion                                 │
├──────────────────────────────────────────────────────────────┤
│ Approval: [1. Tech Lead ✓] ──> [2. QA Sign-off (pending)]   │
│                                                               │
│ [Request Changes]    [Approve]    [Reject]                    │
└──────────────────────────────────────────────────────────────┘
```

### 6. Environment Pipeline

Horizontal stepper showing each configured stage. Each stage card shows: deployed changeset, deployment time, health status. Below: pending deployments (approved changesets ready to promote) + deployment history with log links.

### 7. Settings Page

Two configuration sections:
- **Approval Stages**: ordered list of stages, each with name + required approver count + approver roles. Drag to reorder.
- **Environment Stages**: ordered list of environments (Dev → QA → UAT → Prod), each with name + auto-deploy toggle. Drag to reorder.

---

## Changeset States

```
Draft ──[Submit]──> Submitted ──[Start Review]──> In Review
                                                     │
                                          ┌──────────┼──────────┐
                                          │          │          │
                                    [Approve]  [Req Changes] [Reject]
                                          │          │          │
                                          v          │          v
                                      Approved       │      Rejected
                                          │          │
                                      [Merge]   (back to Draft)
                                          │
                                          v
                                       Merged
```

- **Draft**: Author edits freely. Can submit or delete.
- **Submitted**: Awaiting reviewer. Author can still edit or withdraw.
- **In Review**: Reviewer sees diffs + AI analysis. Can approve, request changes, or reject.
- **Approved**: Ready to merge/deploy. Author or deployer triggers merge.
- **Rejected**: Terminal. Can be cloned into a new changeset.
- **Merged**: Config changes applied to target environment. Terminal.

---

## Editing Modes (Phased)

### Phase 1 (MVP): YAML Text Editor
- Shiki-based syntax highlighting (reusing Studio's `CodeBlock` pattern)
- Editable textarea overlay or upgrade to Monaco editor
- Auto-save (debounced 1s), each save tracked as a changeset edit

### Phase 1 (MVP): AI Chat
- Right panel using `useChat` from `@ai-sdk/react` + `PromptInput`/`Message` from design system
- AI makes changes to files within the changeset context
- Same tool infrastructure as Studio (`readFile`, `writeFile`, `listDir`, `loadSkill`)
- System prompt scoped to the changeset's file snapshot

### Phase 2+: Visual Editor
- Structured forms for entities (field list with type/required/relation dropdowns)
- Page builder (field list with layout preview)
- Queue column configurator
- Provider connection editor

---

## Data Model (MongoDB)

### New Collections

**`applications`** — registered DxFlow config projects
```
{ _id, name, description, configPath, approvalStages[], environmentStages[], createdAt }
```

**`changesets`** — working copies with tracked changes
```
{ _id, appId, number, title, description, status, authorId,
  sourceEnvironment, baseSnapshotId, aiAnalysis?, createdAt, updatedAt }
```

**`changeset_snapshots`** — frozen copy of config at changeset creation
```
{ _id, changesetId, appId, files: [{ path, content, hash }], createdAt }
```

**`changeset_files`** — current state of each file in the changeset
```
{ _id, changesetId, path, content, originalContent, status: unchanged|modified|added|deleted }
```

**`reviews`** — approval decisions per stage
```
{ _id, changesetId, reviewerId, stage, decision, comment?, createdAt }
```

**`comments`** — changeset and inline file comments
```
{ _id, changesetId, authorId, body, filePath?, lineNumber?, resolved, createdAt }
```

**`deployments`** — environment deployment history
```
{ _id, appId, environmentName, changesetId, status, deployedAt, logs? }
```

---

## API Endpoints

```
# Applications
GET    /api/apps                                    List applications
POST   /api/apps                                    Register application
GET    /api/apps/:appId                             Get application detail
PUT    /api/apps/:appId/settings                    Update approval/env stages

# Changesets
GET    /api/apps/:appId/changesets                  List (filter by status)
POST   /api/apps/:appId/changesets                  Create (captures base snapshot)
GET    /api/apps/:appId/changesets/:csId             Detail + file tree
PATCH  /api/apps/:appId/changesets/:csId             Update title/description/status
DELETE /api/apps/:appId/changesets/:csId             Delete draft

# Changeset Files
GET    /api/apps/:appId/changesets/:csId/files       File tree with change markers
GET    /api/apps/:appId/changesets/:csId/files/:path  File content
PUT    /api/apps/:appId/changesets/:csId/files/:path  Save file edit
DELETE /api/apps/:appId/changesets/:csId/files/:path  Mark file deleted

# Diffs
GET    /api/apps/:appId/changesets/:csId/diff         Raw diff (all changed files)
GET    /api/apps/:appId/changesets/:csId/semantic-diff Semantic diff

# AI
POST   /api/apps/:appId/changesets/:csId/analyze      Generate AI analysis
POST   /api/apps/:appId/changesets/:csId/chat          AI chat (streaming)

# Reviews
POST   /api/apps/:appId/changesets/:csId/submit        Submit for review
POST   /api/apps/:appId/changesets/:csId/review        Approve/reject/request changes
GET    /api/apps/:appId/changesets/:csId/comments       List comments
POST   /api/apps/:appId/changesets/:csId/comments       Add comment

# Environments
GET    /api/apps/:appId/environments                   List with deployed versions
POST   /api/apps/:appId/environments/:envId/deploy     Deploy changeset
GET    /api/apps/:appId/deployments                    Deployment history
```

---

## Component Hierarchy

```
App
└─ RouterProvider
   ├─ DashboardPage
   │  └─ AppCard[] + CreateAppDialog
   │
   └─ AppLayout (sidebar + breadcrumbs + outlet)
      ├─ AppOverviewPage
      │  ├─ EnvironmentPipeline
      │  ├─ RecentChangesetsList
      │  └─ QuickActions
      │
      ├─ ChangesetsPage
      │  ├─ StatusFilterTabs
      │  └─ ChangesetCard[]
      │
      ├─ ChangesetWorkspacePage               ← the core screen
      │  └─ ResizablePanelGroup
      │     ├─ FileTreePanel (with change markers)
      │     ├─ EditorPanel
      │     │  ├─ EditorTabs: [Edit | Diff | AI Summary]
      │     │  │  ├─ YamlEditor (CodeBlock + editable textarea)
      │     │  │  ├─ DiffViewer (raw/semantic toggle)
      │     │  │  │  ├─ RawDiffView (unified/split)
      │     │  │  │  └─ SemanticDiffView (grouped by config type)
      │     │  │  └─ AIAnalysisPanel (summary + impact + suggestions)
      │     │  └─ EditorToolbar (save, discard, submit)
      │     └─ ChatPanel (useChat + PromptInput + Message)
      │
      ├─ ChangesetReviewPage
      │  ├─ ReviewHeader (metadata + status)
      │  ├─ ReviewTabs: [Overview | Files | AI Analysis | Comments]
      │  │  ├─ OverviewTab (description + semantic summary + AI excerpt)
      │  │  ├─ FilesTab (expandable per-file diffs + inline comments)
      │  │  ├─ AIAnalysisTab (full analysis)
      │  │  └─ CommentsTab (threaded discussion)
      │  ├─ ApprovalProgress (stage stepper)
      │  └─ ReviewActions (approve / request changes / reject)
      │
      ├─ EnvironmentsPage
      │  ├─ EnvironmentPipeline (horizontal stepper)
      │  ├─ PendingDeployments
      │  └─ DeploymentHistory
      │
      ├─ EnvironmentDetailPage
      │  ├─ DeployedConfig (read-only file tree)
      │  └─ DeploymentLogs
      │
      └─ AppSettingsPage
         ├─ ApprovalStageConfig (drag-to-reorder list)
         └─ EnvironmentStageConfig (drag-to-reorder list)
```

---

## Semantic Diff Engine

Server-side module that parses both base and changed YAML, then produces structured change descriptions per config type:

| Config Type | What's Compared |
|-------------|----------------|
| **Entities** | Fields (added/removed/type changed), relations, hooks, partials |
| **Pages** | Fields (added/removed/reordered), display layout, datasources, roles, actions |
| **Queues** | Columns, filters, sort order, actions, datasources |
| **Providers** | Type, endpoints, auth config, relations, pagination |
| **Workflows** | States (added/removed), transitions, rules, types |
| **Tenant** | App name, domains, roles, branding |
| **Menus** | Items, groups, ordering, role visibility |

Output format:
```typescript
interface SemanticChange {
  configType: 'entity' | 'page' | 'queue' | 'provider' | 'workflow' | 'tenant' | 'menu';
  operation: 'added' | 'modified' | 'removed';
  target: string;        // e.g., "patient", "admin-orders"
  description: string;   // e.g., "Added field `email` (type: string, required)"
  filePath: string;
  lineRange?: [number, number];
  details?: Record<string, unknown>;
}
```

---

## Tech Stack (matches Studio)

- **Frontend:** React 19, TypeScript, Vite, TanStack Router, Tailwind CSS v4, Gistia Design System
- **Backend:** Express 5, MongoDB (native driver), Vercel AI SDK, OpenRouter
- **Diff:** `jsdiff` library for raw diff computation
- **Editor:** Shiki CodeBlock initially → Monaco Editor in Phase 2
- **AI Chat:** `@ai-sdk/react` `useChat` hook with streaming

---

## Phasing

### Phase 1 — MVP Foundation (scaffold + CRUD + editor + AI chat)
1. Scaffold new Vite + React 19 + TanStack Router app
2. Dashboard page with application list
3. Application shell (sidebar + breadcrumbs)
4. Application overview page (static pipeline + changeset list)
5. Changeset creation (snapshot base config from disk)
6. **Changeset workspace** — file tree + YAML editor + AI chat panel
7. Changeset list with status filters
8. Backend: Express API + MongoDB for apps, changesets, snapshots, files, chat

### Phase 2 — Diff & Review
1. Raw YAML diff view (unified + split modes, using `jsdiff`)
2. Semantic diff engine (YAML-aware parsing per config type)
3. Toggle between raw/semantic views
4. AI analysis endpoint (summary + impact + suggestions)
5. Review page with tabs (Overview, Files, AI Analysis, Comments)
6. Inline file commenting
7. Approval workflow (configurable stages, approve/reject/request changes)

### Phase 3 — Environments & Deployment
1. Environment configuration (settings page, drag-to-reorder stages)
2. Environment pipeline visualization
3. Deploy action (merge changeset to environment's config)
4. Promote action (advance through stages)
5. Deployment history + status tracking
6. Auto-deploy on merge (optional per environment)

### Phase 4 — Polish & Visual Editor
1. Upgrade to Monaco Editor (autocomplete, validation, error markers)
2. Visual editor for entities (form-based field management)
3. Visual editor for pages and queues
4. Notifications (review requests, status changes)
5. User management and RBAC

---

## Verification Plan

1. **Phase 1 verification:** Create an application pointing to `detoxu-config`, create a changeset, edit `config/entities/patient.yml` via YAML editor, edit via AI chat ("add email field to patient"), confirm file tree shows `[M]` marker, confirm saves persist
2. **Phase 2 verification:** View raw diff showing added lines in green, view semantic diff showing "Entity patient: Added field email", generate AI analysis and verify summary/impact/suggestions render correctly, submit for review, approve as reviewer
3. **Phase 3 verification:** Configure Dev → QA → Prod pipeline, deploy approved changeset to Dev, verify config files updated on disk, promote to QA, verify deployment history shows both entries

---

## Key Files to Create

```
# New application root (sibling to poc-dxflow-studio)
config-manager/
├── src/
│   ├── App.tsx
│   ├── main.tsx
│   ├── routeTree.gen.tsx
│   ├── index.css                          # Copy Studio's Tailwind/theme setup
│   ├── components/
│   │   ├── app-layout.tsx                 # Sidebar shell (adapted from Studio)
│   │   ├── theme-provider.tsx             # Copy from Studio
│   │   └── environment-pipeline.tsx       # Reusable horizontal stepper
│   ├── pages/
│   │   ├── Dashboard.tsx
│   │   ├── AppOverview.tsx
│   │   ├── changeset/
│   │   │   ├── ChangesetList.tsx
│   │   │   ├── ChangesetWorkspace.tsx     # Main editor (3-panel layout)
│   │   │   ├── ChangesetReview.tsx
│   │   │   ├── changeset-file-tree.tsx    # File tree with change markers
│   │   │   ├── changeset-yaml-editor.tsx
│   │   │   ├── changeset-chat-panel.tsx
│   │   │   ├── raw-diff-viewer.tsx
│   │   │   ├── semantic-diff-viewer.tsx
│   │   │   ├── ai-analysis-panel.tsx
│   │   │   ├── review-actions.tsx
│   │   │   ├── inline-comment.tsx
│   │   │   └── changeset-shared.tsx       # Types + utilities
│   │   ├── environment/
│   │   │   ├── EnvironmentsPage.tsx
│   │   │   └── EnvironmentDetail.tsx
│   │   └── settings/
│   │       └── AppSettings.tsx
│   ├── lib/
│   │   └── utils.ts                       # cn() helper (copy from Studio)
│   ├── hooks/
│   │   └── use-mobile.ts                  # Copy from Studio
│   └── types/
│       └── gistia-design-system.d.ts      # Copy from Studio
├── server/
│   └── src/
│       ├── index.ts
│       ├── db.ts                          # MongoDB collections
│       ├── model.ts                       # OpenRouter config (copy pattern)
│       ├── tools.ts                       # AI tools scoped to changeset
│       ├── skills.ts                      # Skill system (copy from Studio)
│       ├── semantic-diff.ts               # YAML-aware diff engine
│       └── routes/
│           ├── apps.ts
│           ├── changesets.ts
│           ├── reviews.ts
│           ├── environments.ts
│           └── chat.ts
├── package.json
├── vite.config.ts                         # Same pattern as Studio
├── tsconfig.json
└── components.json                        # shadcn/ui config
```

## Patterns to Reuse from Studio

| Pattern | Studio File | Reuse In |
|---------|------------|----------|
| Sidebar shell | `src/components/app-layout.tsx` | `components/app-layout.tsx` |
| Theme provider | `src/components/theme-provider.tsx` | `components/theme-provider.tsx` |
| Resizable 3-panel workspace | `src/pages/Project.tsx` | `changeset/ChangesetWorkspace.tsx` |
| File tree with markers | `src/pages/project/project-workspace-tabs.tsx` | `changeset/changeset-file-tree.tsx` |
| AI chat panel | `src/pages/project/project-chat-panel.tsx` | `changeset/changeset-chat-panel.tsx` |
| Shared types + utilities | `src/pages/project/project-shared.tsx` | `changeset/changeset-shared.tsx` |
| AI tools (readFile, writeFile, etc.) | `server/src/tools.ts` | `server/src/tools.ts` |
| Skills system | `server/src/skills.ts` | `server/src/skills.ts` |
| MongoDB patterns | `server/src/db.ts` | `server/src/db.ts` |
| Vite + proxy config | `vite.config.ts` | `vite.config.ts` |

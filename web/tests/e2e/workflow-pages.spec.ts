import { expect, test, type Page, type Route } from "@playwright/test";

type JsonValue = Record<string, unknown> | unknown[];
type MockRole = "member" | "reviewer" | "config_manager" | "admin" | "owner";

function ok(data: JsonValue, pagination?: { page: number; limit: number; total: number }) {
  return { data, ...(pagination ? { pagination } : {}) };
}

async function json(route: Route, status: number, body: unknown) {
  await route.fulfill({
    status,
    contentType: "application/json",
    body: JSON.stringify(body),
  });
}

async function setupApiMock(page: Page, options?: { role?: MockRole; canRebind?: boolean }): Promise<void> {
  const role = options?.role ?? "owner";
  const canRebind = options?.canRebind ?? (role === "admin" || role === "owner");
  let invites = [
    {
      id: "inv-1",
      team_id: "team-1",
      email: "reviewer@example.com",
      role: "reviewer",
      token: "token-1",
      invited_by: "owner-1",
      expires_at: "2026-12-01T00:00:00Z",
      accepted_at: null,
      created_at: "2026-02-01T00:00:00Z",
    },
  ];

  const members = [
    {
      user_id: "owner-1",
      repo_id: "repo-1",
      role: "owner",
      created_at: "2026-01-01T00:00:00Z",
      email: "owner@example.com",
      name: "Owner",
    },
    {
      user_id: "reviewer-1",
      repo_id: "repo-1",
      role: "reviewer",
      created_at: "2026-01-02T00:00:00Z",
      email: "reviewer@example.com",
      name: "Reviewer",
    },
  ];

  const releases = [
    {
      id: "rel-1",
      repo_id: "repo-1",
      tag: "r2026.02.28.1",
      state: "draft_release",
      ordered_changeset_ids: ["cs-1"],
      compose_job_id: null,
      published_sha: null,
      published_at: null,
      published_by: null,
      created_at: "2026-02-28T00:00:00Z",
      updated_at: "2026-02-28T00:00:00Z",
    },
  ];

  const changesets = [
    {
      id: "cs-1",
      repo_id: "repo-1",
      workspace_id: "ws-1",
      title: "Update app config",
      description: "Main path update",
      state: "queued",
      author_user_id: "owner-1",
      head_sha: "abc",
      revision: 1,
      approvals: [],
      created_at: "2026-02-28T00:00:00Z",
      updated_at: "2026-02-28T00:00:00Z",
    },
    {
      id: "cs-2",
      repo_id: "repo-1",
      workspace_id: "ws-2",
      title: "Hotfix route rules",
      description: "Routing update",
      state: "queued",
      author_user_id: "reviewer-1",
      head_sha: "def",
      revision: 1,
      approvals: [],
      created_at: "2026-02-28T00:00:00Z",
      updated_at: "2026-02-28T00:00:00Z",
    },
  ];

  const deployments = [
    {
      id: "dep-run",
      repo_id: "repo-1",
      environment_id: "env-dev",
      release_id: "rel-run",
      state: "running",
      is_skip_stage: false,
      is_concurrent_batch: false,
      approvals: [],
      created_by: "owner-1",
      created_at: "2026-02-28T00:00:02Z",
      updated_at: "2026-02-28T00:00:03Z",
    },
    {
      id: "dep-ok",
      repo_id: "repo-1",
      environment_id: "env-stage",
      release_id: "rel-ok",
      state: "succeeded",
      is_skip_stage: false,
      is_concurrent_batch: false,
      approvals: [],
      created_by: "owner-1",
      created_at: "2026-02-28T00:00:04Z",
      updated_at: "2026-02-28T00:00:05Z",
    },
    {
      id: "dep-fail",
      repo_id: "repo-1",
      environment_id: "env-prod",
      release_id: "rel-fail",
      state: "failed",
      is_skip_stage: false,
      is_concurrent_batch: false,
      approvals: [],
      created_by: "owner-1",
      created_at: "2026-02-28T00:00:06Z",
      updated_at: "2026-02-28T00:00:07Z",
    },
  ];

  await page.route("**/*", async (route) => {
    const request = route.request();
    const url = new URL(request.url());
    const pathname = url.pathname;
    const method = request.method();

    if (!pathname.startsWith("/api/")) {
      await route.continue();
      return;
    }

    if (pathname === "/api/repo" && method === "GET") {
      return json(
        route,
        200,
        ok({
          status: "bound",
          binding: {
            id: "owner-1",
            repo_id: "repo-1",
            configured_by: "owner-1",
            configured_at: "2026-02-01T00:00:00Z",
            updated_at: "2026-02-01T00:00:00Z",
          },
          repo: {
            id: "repo-1",
            team_id: "team-1",
            name: "Main Instance",
            repo_path: "main-instance",
            integration_branch: "main",
            settings: {
              baseline_mode: "branch",
              commit_mode_default: "patch",
              blocked_paths: [],
              file_size_limit_bytes: 1000000,
              profile_approval_policy: "same_as_changeset",
            },
            created_by: "owner-1",
            created_at: "2026-01-01T00:00:00Z",
            updated_at: "2026-01-01T00:00:00Z",
          },
          team: { id: "team-1", name: "Team One", slug: "team-one" },
          apps: [],
          role,
          can_rebind: canRebind,
        }),
      );
    }

    if (pathname === "/api/repo" && method === "PATCH") {
      return json(route, 200, ok({ success: true }));
    }

    if (pathname === "/api/teams" && method === "GET") {
      return json(route, 200, ok([{ id: "team-1", name: "Team One", slug: "team-one" }], { page: 1, limit: 500, total: 1 }));
    }

    if (pathname === "/api/repos" && method === "GET") {
      return json(
        route,
        200,
        ok(
          [
            {
              id: "repo-1",
              team_id: "team-1",
              name: "Main Instance",
              repo_path: "main-instance",
              integration_branch: "main",
              settings: {
                baseline_mode: "branch",
                commit_mode_default: "patch",
                blocked_paths: [],
                file_size_limit_bytes: 1000000,
                profile_approval_policy: "same_as_changeset",
              },
              created_by: "owner-1",
              created_at: "2026-01-01T00:00:00Z",
              updated_at: "2026-01-01T00:00:00Z",
            },
          ],
          { page: 1, limit: 500, total: 1 },
        ),
      );
    }

    if (pathname === "/api/repos/repo-1/releases" && method === "GET") {
      return json(route, 200, ok(releases, { page: 1, limit: 100, total: releases.length }));
    }

    if (pathname === "/api/repos/repo-1/releases" && method === "POST") {
      const created = {
        ...releases[0],
        id: `rel-${releases.length + 1}`,
        tag: `r2026.02.28.${releases.length + 1}`,
      };
      releases.unshift(created);
      return json(route, 200, ok(created));
    }

    if (pathname.startsWith("/api/repos/repo-1/releases/") && method === "POST") {
      return json(route, 200, ok({ success: true }));
    }

    if (pathname === "/api/repos/repo-1/changesets" && method === "GET") {
      return json(route, 200, ok(changesets, { page: 1, limit: 200, total: changesets.length }));
    }

    if (pathname.startsWith("/api/repos/repo-1/changesets/") && pathname.endsWith("/diff") && method === "GET") {
      return json(route, 200, ok({
        changeset: pathname.split("/")[5],
        files: [{ path: "config/app.yml" }, { path: "routes/checkout.yml" }],
      }));
    }

    if (pathname === "/api/repos/repo-1/deployments" && method === "GET") {
      return json(route, 200, ok(deployments));
    }

    if (pathname === "/api/repos/repo-1/environments" && method === "GET") {
      return json(route, 200, ok([
        { id: "env-dev", name: "Development" },
        { id: "env-stage", name: "Staging" },
        { id: "env-prod", name: "Production" },
      ]));
    }

    if (pathname.match(/^\/api\/repos\/repo-1\/environments\/[^/]+\/(deploy|promote|rollback)$/) && method === "POST") {
      return json(route, 200, ok({ success: true }));
    }

    if (pathname === "/api/repos/repo-1/members" && method === "GET") {
      return json(route, 200, ok(members, { page: 1, limit: 100, total: members.length }));
    }

    if (pathname === "/api/repos/repo-1/members" && method === "POST") {
      return json(route, 200, ok({ success: true }));
    }

    if (pathname === "/api/teams/team-1/invites" && method === "GET") {
      return json(route, 200, ok(invites, { page: 1, limit: 100, total: invites.length }));
    }

    if (pathname === "/api/teams/team-1/invites" && method === "POST") {
      const body = JSON.parse(request.postData() || "{}");
      invites = [
        {
          id: `inv-${invites.length + 1}`,
          team_id: "team-1",
          email: String(body.email || "new@example.com"),
          role: String(body.role || "member"),
          token: `token-${invites.length + 1}`,
          invited_by: "owner-1",
          expires_at: "2026-12-01T00:00:00Z",
          accepted_at: null,
          created_at: "2026-02-01T00:00:00Z",
        },
        ...invites,
      ];
      return json(route, 200, ok(invites[0]));
    }

    if (pathname.match(/^\/api\/teams\/team-1\/invites\/[^/]+\/resend$/) && method === "POST") {
      return json(route, 200, ok({ success: true }));
    }

    if (pathname.match(/^\/api\/teams\/team-1\/invites\/[^/]+$/) && method === "DELETE") {
      const inviteId = pathname.split("/").pop();
      invites = invites.filter((invite) => invite.id !== inviteId);
      return json(route, 200, ok({ success: true }));
    }

    return json(route, 404, {
      error: {
        code: "not_found",
        message: `No mock for ${method} ${pathname}`,
        request_id: "mock-404",
      },
    });
  });
}

async function authenticate(page: Page): Promise<void> {
  await page.goto("/login");
  await page.evaluate(() => {
    localStorage.setItem("conman.auth.token", "test-token");
  });
  await page.reload();
}

test("releases page shows impact summary for selected changesets", async ({ page }) => {
  await setupApiMock(page);
  await authenticate(page);
  await page.goto("/releases");

  await expect(page.getByRole("heading", { name: "Releases" })).toBeVisible();
  await page.getByRole("button", { name: "Refresh impact summary" }).click();

  await expect(page.getByText("Detected changed paths")).toBeVisible();
  await expect(page.locator("li", { hasText: "config/app.yml" }).first()).toBeVisible();
});

test("deployments history supports state filtering and detail selection", async ({ page }) => {
  await setupApiMock(page);
  await authenticate(page);
  await page.goto("/deployments");

  await expect(page.getByRole("heading", { name: "Deployments" })).toBeVisible();
  await page.getByLabel("State filter").selectOption("failed");

  await expect(page.getByText("release rel-fail").first()).toBeVisible();
  await expect(page.getByText("state: failed")).toBeVisible();
});

test("members and settings pages use guided admin flows", async ({ page }) => {
  await setupApiMock(page, { role: "owner" });
  await authenticate(page);

  await page.goto("/members");
  await expect(page.getByRole("heading", { name: "Members & Invites" })).toBeVisible();

  await page.getByLabel("Email").fill("new-user@example.com");
  await page.getByRole("button", { name: "Send invite" }).click();
  await expect(page.getByText("Invite created.")).toBeVisible();
  await expect(page.getByText("new-user@example.com")).toBeVisible();

  await page.goto("/settings");
  await expect(page.getByRole("heading", { name: "Settings" })).toBeVisible();
  await expect(page.getByLabel("Team")).toBeVisible();
  await expect(page.getByLabel("Instance")).toBeVisible();
  await page.getByRole("button", { name: "Apply instance binding" }).click();
  await expect(page.getByText("Bound instance updated.")).toBeVisible();
});

test("reviewer nav and access excludes release and admin sections", async ({ page }) => {
  await setupApiMock(page, { role: "reviewer", canRebind: false });
  await authenticate(page);
  await page.goto("/workspaces");

  const nav = page.getByRole("navigation");

  await expect(page.getByRole("heading", { name: "Draft Changes" })).toBeVisible();
  await expect(nav.getByRole("link", { name: "Draft Changes" })).toBeVisible();
  await expect(nav.getByRole("link", { name: "Changesets" })).toBeVisible();
  await expect(nav.getByRole("link", { name: "Preview Envs" })).toBeVisible();
  await expect(nav.getByRole("link", { name: "Releases" })).toHaveCount(0);
  await expect(nav.getByRole("link", { name: "Deployments" })).toHaveCount(0);
  await expect(nav.getByRole("link", { name: "Members" })).toHaveCount(0);
});

test("config manager nav includes release and operations but excludes admin", async ({ page }) => {
  await setupApiMock(page, { role: "config_manager", canRebind: false });
  await authenticate(page);
  await page.goto("/workspaces");

  const nav = page.getByRole("navigation");

  await expect(nav.getByRole("link", { name: "Releases" })).toBeVisible();
  await expect(nav.getByRole("link", { name: "Deployments" })).toBeVisible();
  await expect(nav.getByRole("link", { name: "Runtime" })).toBeVisible();
  await expect(nav.getByRole("link", { name: "Members" })).toHaveCount(0);
  await expect(nav.getByRole("link", { name: "Settings" })).toHaveCount(0);
});

test("member cannot open release routes", async ({ page }) => {
  await setupApiMock(page, { role: "member", canRebind: false });
  await authenticate(page);
  await page.goto("/releases");

  await expect(page.getByRole("heading", { name: "Access denied" })).toBeVisible();
  await expect(page.getByText("requires config_manager role or higher")).toBeVisible();
});

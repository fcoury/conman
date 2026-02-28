import { Navigate, Outlet, Route, Routes, useLocation } from "react-router-dom";

import { ApiError } from "@/api/client";
import { AppShell } from "@/components/layout/app-shell";
import { useAuth } from "@/hooks/use-auth";
import { RepoContextProvider, useRepoContextQuery } from "@/hooks/use-repo-context";
import { hasMinimumRole } from "@/lib/rbac";
import { AcceptInvitePage } from "@/modules/auth/accept-invite-page";
import { AccessDeniedPage } from "@/modules/auth/access-denied-page";
import { ForgotPasswordPage } from "@/modules/auth/forgot-password-page";
import { LoginPage } from "@/modules/auth/login-page";
import { ResetPasswordPage } from "@/modules/auth/reset-password-page";
import { SignupPage } from "@/modules/auth/signup-page";
import { AppsPage } from "@/modules/apps/apps-page";
import { ChangesetsPage } from "@/modules/changesets/changesets-page";
import { DeploymentsPage } from "@/modules/deployments/deployments-page";
import { JobsPage } from "@/modules/jobs/jobs-page";
import { MembersPage } from "@/modules/members/members-page";
import { NotificationsPage } from "@/modules/notifications/notifications-page";
import { ReleasesPage } from "@/modules/releases/releases-page";
import { RuntimePage } from "@/modules/runtime/runtime-page";
import { SettingsPage } from "@/modules/settings/settings-page";
import { SetupPage } from "@/modules/setup/setup-page";
import { LoadingPanel } from "@/modules/shared/loading-panel";
import { NotFoundPage } from "@/modules/shared/not-found-page";
import { TempEnvsPage } from "@/modules/temp-envs/temp-envs-page";
import { WorkspacesPage } from "@/modules/workspaces/workspaces-page";
import type { Role } from "@/types/api";

function RequireAuth(): React.ReactElement {
  const { isAuthenticated } = useAuth();
  const location = useLocation();

  if (!isAuthenticated) {
    return <Navigate to="/login" state={{ from: location }} replace />;
  }

  return <Outlet />;
}

function ProtectedAppLayout(): React.ReactElement {
  const location = useLocation();
  const { logout } = useAuth();
  const contextQuery = useRepoContextQuery();

  if (contextQuery.isLoading) {
    return (
      <div className="flex h-screen items-center justify-center bg-background">
        <LoadingPanel label="Loading repo context..." />
      </div>
    );
  }

  if (contextQuery.error instanceof ApiError) {
    if (contextQuery.error.status === 401) {
      logout();
      return <Navigate to="/login" replace />;
    }
    if (contextQuery.error.status === 403) {
      return <AccessDeniedPage message={contextQuery.error.message} />;
    }
    return <AccessDeniedPage message={contextQuery.error.message} />;
  }

  const context = contextQuery.data ?? null;
  if (context?.status === "unbound" && location.pathname !== "/setup") {
    return <Navigate to="/setup" replace />;
  }

  return (
    <RepoContextProvider value={context}>
      <AppShell>
        <Outlet />
      </AppShell>
    </RepoContextProvider>
  );
}

function RequireRole({
  minimum,
  children,
}: {
  minimum: Role;
  children: React.ReactElement;
}): React.ReactElement {
  const contextQuery = useRepoContextQuery();
  const role = contextQuery.data?.role;

  if (!hasMinimumRole(role, minimum)) {
    return <AccessDeniedPage message={`requires ${minimum} role or higher`} />;
  }

  return children;
}

function IndexRoute(): React.ReactElement {
  const { isAuthenticated } = useAuth();
  if (!isAuthenticated) {
    return <Navigate to="/login" replace />;
  }
  return <Navigate to="/workspaces" replace />;
}

export function AppRoutes(): React.ReactElement {
  return (
    <Routes>
      <Route path="/" element={<IndexRoute />} />
      <Route path="/login" element={<LoginPage />} />
      <Route path="/signup" element={<SignupPage />} />
      <Route path="/forgot-password" element={<ForgotPasswordPage />} />
      <Route path="/reset-password" element={<ResetPasswordPage />} />
      <Route path="/accept-invite" element={<AcceptInvitePage />} />

      <Route element={<RequireAuth />}>
        {/* Setup wizard renders full-screen, outside app shell */}
        <Route path="/setup" element={<SetupPage />} />

        <Route element={<ProtectedAppLayout />}>
          <Route path="/workspaces" element={<WorkspacesPage />} />
          <Route path="/changesets" element={<ChangesetsPage />} />
          <Route
            path="/releases"
            element={
              <RequireRole minimum="config_manager">
                <ReleasesPage />
              </RequireRole>
            }
          />
          <Route
            path="/deployments"
            element={
              <RequireRole minimum="config_manager">
                <DeploymentsPage />
              </RequireRole>
            }
          />
          <Route
            path="/runtime"
            element={
              <RequireRole minimum="config_manager">
                <RuntimePage />
              </RequireRole>
            }
          />
          <Route path="/temp-envs" element={<TempEnvsPage />} />
          <Route path="/jobs" element={<JobsPage />} />
          <Route
            path="/apps"
            element={
              <RequireRole minimum="admin">
                <AppsPage />
              </RequireRole>
            }
          />
          <Route
            path="/members"
            element={
              <RequireRole minimum="admin">
                <MembersPage />
              </RequireRole>
            }
          />
          <Route path="/notifications" element={<NotificationsPage />} />
          <Route
            path="/settings"
            element={
              <RequireRole minimum="admin">
                <SettingsPage />
              </RequireRole>
            }
          />
        </Route>
      </Route>

      <Route path="*" element={<NotFoundPage />} />
    </Routes>
  );
}

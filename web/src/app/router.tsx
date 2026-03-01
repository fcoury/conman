import { Navigate, Route, Routes } from 'react-router-dom';

import DashboardPage from '~/modules/dashboard/dashboard-page';
import LoginPage from '~/modules/auth/login-page';
import SignupPage from '~/modules/auth/signup-page';
import { PublicOnlyRoute, RequireAuthRoute } from '~/modules/auth/auth-routes';
import InstancesPage from '~/modules/instances/instances-page';
import { TeamContextLayout } from '~/modules/teams/team-context';
import RequireTeamSelectionRoute from '~/modules/teams/require-team-selection-route';
import TeamPickerPage from '~/modules/teams/team-picker-page';

import AppTemplate from './components/template/app-template';

export default function Router() {
  return (
    <Routes>
      <Route element={<PublicOnlyRoute />}>
        <Route path="login" element={<LoginPage />} />
        <Route path="signup" element={<SignupPage />} />
      </Route>

      <Route element={<RequireAuthRoute />}>
        <Route element={<TeamContextLayout />}>
          <Route path="select-team" element={<TeamPickerPage />} />

          <Route element={<RequireTeamSelectionRoute />}>
            <Route element={<AppTemplate />}>
              <Route index element={<DashboardPage />} />
              <Route path="instances" element={<InstancesPage />} />
              <Route path="repos" element={<Navigate to="/instances" replace />} />
            </Route>
          </Route>
        </Route>
      </Route>

      <Route path="*" element={<Navigate to="/" replace />} />
    </Routes>
  );
}

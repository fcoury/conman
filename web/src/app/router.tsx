import { Navigate, Route, Routes } from 'react-router-dom';

import DashboardPage from '~/modules/dashboard/dashboard-page';
import ReposPage from '~/modules/repos/repos-page';

import AppTemplate from './components/template/app-template';

export default function Router() {
  return (
    <Routes>
      <Route element={<AppTemplate />}>
        <Route index element={<DashboardPage />} />
        <Route path="repos" element={<ReposPage />} />
        <Route path="*" element={<Navigate to="/" replace />} />
      </Route>
    </Routes>
  );
}

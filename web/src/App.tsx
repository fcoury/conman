import { ThemeProvider } from 'next-themes';
import { Toaster } from 'sonner';

import { AuthProvider } from './modules/auth/auth-context';
import Router from './app/router';

export default function App() {
  return (
    <ThemeProvider
      attribute="class"
      defaultTheme="light"
      enableSystem={false}
      storageKey="theme"
    >
      <AuthProvider>
        <Router />
      </AuthProvider>
      <Toaster richColors position="top-center" />
    </ThemeProvider>
  );
}

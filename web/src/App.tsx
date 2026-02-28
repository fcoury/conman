import { Toaster } from 'gistia-design-system';
import { ThemeProvider } from 'next-themes';

import Router from './app/router';

export default function App() {
  return (
    <ThemeProvider
      attribute="class"
      defaultTheme="light"
      enableSystem={false}
      storageKey="theme"
    >
      <Router />
      <Toaster richColors position="top-center" />
    </ThemeProvider>
  );
}

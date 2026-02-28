import {
  Separator,
  SidebarInset,
  SidebarProvider,
  SidebarTrigger,
} from 'gistia-design-system';
import { Outlet } from 'react-router-dom';

import AppSidebar from './app-sidebar';

export default function AppTemplate() {
  return (
    <SidebarProvider>
      <AppSidebar />
      <SidebarInset>
        <header className="flex h-16 shrink-0 items-center gap-2 border-b px-4">
          <SidebarTrigger className="-ml-1" />
          <Separator orientation="vertical" className="mr-2 h-4" />
          <span className="text-sm text-muted-foreground">Conman</span>
        </header>
        <main className="flex-1 p-4">
          <Outlet />
        </main>
      </SidebarInset>
    </SidebarProvider>
  );
}

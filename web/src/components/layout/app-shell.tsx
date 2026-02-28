import { useRepoContext } from "@/hooks/use-repo-context";
import { Logo } from "@/components/ui/logo";
import { Separator } from "@/components/ui/separator";
import { SidebarNav } from "@/components/layout/sidebar-nav";
import { UserMenu } from "@/components/layout/user-menu";

export function AppShell({ children }: { children: React.ReactNode }): React.ReactElement {
  const context = useRepoContext();
  const isBound = context?.status === "bound";

  return (
    <div className="flex h-screen">
      {/* Sidebar */}
      <aside className="fixed inset-y-0 left-0 z-30 w-60 flex flex-col border-r border-border bg-sidebar">
        <div className="flex h-14 items-center px-4">
          <Logo size="sm" />
        </div>
        <Separator />
        <div className="flex-1 overflow-y-auto py-2">
          {isBound ? (
            <SidebarNav />
          ) : (
            <div className="px-5 py-4 text-sm text-muted-foreground">
              Setup required
            </div>
          )}
        </div>
        {isBound && context?.repo && (
          <>
            <Separator />
            <div className="px-4 py-3">
              <span className="text-xs text-muted-foreground truncate block">
                {context.repo.name}
              </span>
            </div>
          </>
        )}
      </aside>

      {/* Main content area */}
      <div className="ml-60 flex flex-1 flex-col">
        <header className="sticky top-0 z-20 flex h-14 items-center justify-end border-b border-border bg-background/80 px-6 backdrop-blur">
          <UserMenu />
        </header>
        <main className="flex-1 overflow-auto px-6 py-6">
          {children}
        </main>
      </div>
    </div>
  );
}

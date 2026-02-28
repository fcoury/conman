import { Link } from "react-router-dom";

import { SidebarNav } from "@/components/layout/sidebar-nav";
import { UserMenu } from "@/components/layout/user-menu";
import { Separator } from "@/components/ui/separator";
import { Logo } from "@/components/ui/logo";
import { useRepoContext } from "@/hooks/use-repo-context";
import { formatRoleLabel } from "@/lib/rbac";

export function AppShell({ children }: { children: React.ReactNode }): React.ReactElement {
  const context = useRepoContext();
  const isBound = context?.status === "bound";

  return (
    <div className="flex h-screen">
      <aside className="fixed inset-y-0 left-0 z-30 flex w-60 flex-col border-r border-border bg-sidebar">
        <div className="flex h-14 items-center px-4">
          <Logo size="sm" />
        </div>
        <Separator />
        <div className="flex-1 overflow-y-auto py-2">
          {isBound ? (
            <>
              <SidebarNav />
              <div className="mt-4 px-4 text-xs text-muted-foreground">
                Primary flow: Draft changes in <Link className="text-primary underline" to="/workspaces">Draft Changes</Link>,
                then submit in <Link className="text-primary underline" to="/changesets">Changesets</Link>.
              </div>
            </>
          ) : (
            <div className="px-5 py-4 text-sm text-muted-foreground">Setup required</div>
          )}
        </div>
        {isBound && context?.repo && (
          <>
            <Separator />
            <div className="space-y-1 px-4 py-3">
              <span className="block truncate text-xs text-muted-foreground">{context.repo.name}</span>
              <span className="block text-[11px] font-medium uppercase tracking-wide text-muted-foreground">
                {formatRoleLabel(context.role)}
              </span>
            </div>
          </>
        )}
      </aside>

      <div className="ml-60 flex flex-1 flex-col">
        <header className="sticky top-0 z-20 flex h-14 items-center justify-end border-b border-border bg-background/80 px-6 backdrop-blur">
          <UserMenu />
        </header>
        <main className="flex-1 overflow-auto px-6 py-6">{children}</main>
      </div>
    </div>
  );
}

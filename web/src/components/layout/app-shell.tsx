import { NavLink } from "react-router-dom";
import { LayoutDashboard, Settings, UserCircle } from "lucide-react";

import { useAuth } from "@/hooks/use-auth";
import { useRepoContext } from "@/hooks/use-repo-context";
import { cn } from "@/lib/utils";

const navItems = [
  { to: "/workspaces", label: "Workspaces" },
  { to: "/changesets", label: "Changesets" },
  { to: "/releases", label: "Releases" },
  { to: "/deployments", label: "Deployments" },
  { to: "/runtime", label: "Runtime" },
  { to: "/temp-envs", label: "Temp Envs" },
  { to: "/jobs", label: "Jobs" },
  { to: "/apps", label: "Apps" },
  { to: "/members", label: "Members" },
  { to: "/notifications", label: "Notifications" },
  { to: "/settings", label: "Settings" },
];

export function AppShell({ children }: { children: React.ReactNode }): React.ReactElement {
  const { logout } = useAuth();
  const context = useRepoContext();

  return (
    <div className="from-background to-muted/40 min-h-screen bg-gradient-to-br">
      <header className="border-border/60 bg-background/90 sticky top-0 z-20 border-b backdrop-blur">
        <div className="mx-auto flex h-14 max-w-[1600px] items-center justify-between gap-4 px-4">
          <div className="flex items-center gap-2">
            <LayoutDashboard className="h-4 w-4" />
            <span className="text-sm font-semibold">Conman Console</span>
          </div>
          <div className="text-muted-foreground truncate text-xs">
            {context?.repo ? `Repo: ${context.repo.name}` : "No repo bound"}
          </div>
          <button
            onClick={logout}
            className="text-muted-foreground hover:text-foreground inline-flex items-center gap-1 text-xs"
            type="button"
          >
            <UserCircle className="h-4 w-4" /> Logout
          </button>
        </div>
      </header>

      <div className="mx-auto grid max-w-[1600px] grid-cols-1 gap-4 px-4 py-4 lg:grid-cols-[230px_1fr]">
        <aside className="bg-card h-fit rounded-xl border p-2">
          <nav className="space-y-1">
            {navItems.map((item) => (
              <NavLink
                key={item.to}
                to={item.to}
                className={({ isActive }) =>
                  cn(
                    "hover:bg-accent hover:text-accent-foreground flex items-center gap-2 rounded-md px-3 py-2 text-sm",
                    isActive ? "bg-primary text-primary-foreground hover:bg-primary/90 hover:text-primary-foreground" : "",
                  )
                }
              >
                {item.label}
              </NavLink>
            ))}
          </nav>
          <div className="mt-3 border-t pt-3">
            <NavLink
              to="/setup"
              className={({ isActive }) =>
                cn(
                  "hover:bg-accent hover:text-accent-foreground flex items-center gap-2 rounded-md px-3 py-2 text-sm",
                  isActive ? "bg-primary text-primary-foreground" : "",
                )
              }
            >
              <Settings className="h-4 w-4" /> Setup
            </NavLink>
          </div>
        </aside>
        <main className="min-w-0">{children}</main>
      </div>
    </div>
  );
}

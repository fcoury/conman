import { NavLink } from "react-router-dom";
import {
  Bell,
  Cog,
  Eye,
  FlaskConical,
  GitBranch,
  GitPullRequest,
  LayoutGrid,
  Package,
  Rocket,
  Server,
  Settings,
  Users,
} from "lucide-react";

import { useRepoContext } from "@/hooks/use-repo-context";
import { cn } from "@/lib/utils";
import { hasMinimumRole } from "@/lib/rbac";
import type { Role } from "@/types/api";

interface NavItem {
  to: string;
  label: string;
  icon: React.ElementType;
  minRole?: Role;
}

interface NavGroup {
  label: string;
  items: NavItem[];
}

const navGroups: NavGroup[] = [
  {
    label: "Build",
    items: [
      { to: "/workspaces", label: "Draft Changes", icon: GitBranch },
      { to: "/changesets", label: "Changesets", icon: GitPullRequest },
      { to: "/temp-envs", label: "Preview Envs", icon: FlaskConical },
    ],
  },
  {
    label: "Review",
    items: [{ to: "/review", label: "Review Queue", icon: Eye, minRole: "reviewer" }],
  },
  {
    label: "Release",
    items: [
      { to: "/releases", label: "Releases", icon: Package, minRole: "config_manager" },
      { to: "/deployments", label: "Deployments", icon: Rocket, minRole: "config_manager" },
    ],
  },
  {
    label: "Operations",
    items: [
      { to: "/runtime", label: "Runtime", icon: Server, minRole: "config_manager" },
      { to: "/jobs", label: "Jobs", icon: Cog },
    ],
  },
  {
    label: "Administration",
    items: [
      { to: "/apps", label: "Apps", icon: LayoutGrid, minRole: "admin" },
      { to: "/members", label: "Members", icon: Users, minRole: "admin" },
      { to: "/notifications", label: "Notifications", icon: Bell },
      { to: "/settings", label: "Settings", icon: Settings, minRole: "admin" },
    ],
  },
];

export function SidebarNav(): React.ReactElement {
  const context = useRepoContext();
  const role = context?.role;

  const visibleGroups = navGroups
    .map((group) => ({
      ...group,
      items: group.items.filter((item) => !item.minRole || hasMinimumRole(role, item.minRole)),
    }))
    .filter((group) => group.items.length > 0);

  return (
    <nav className="flex flex-col gap-4 px-3 py-2">
      {visibleGroups.map((group) => (
        <div key={group.label}>
          <span className="mb-1 block px-2 text-[11px] font-semibold uppercase tracking-wider text-muted-foreground">
            {group.label}
          </span>
          <div className="space-y-0.5">
            {group.items.map((item) => (
              <NavLink
                key={item.to}
                to={item.to}
                className={({ isActive }) =>
                  cn(
                    "group flex items-center gap-2.5 rounded-md px-2 py-1.5 text-sm transition-colors",
                    isActive
                      ? "-ml-px border-l-[3px] border-sidebar-active bg-sidebar-active/10 text-sidebar-active"
                      : "text-sidebar-foreground hover:bg-accent hover:text-accent-foreground",
                  )
                }
              >
                <item.icon aria-hidden="true" className="h-4 w-4 shrink-0" />
                {item.label}
              </NavLink>
            ))}
          </div>
        </div>
      ))}
    </nav>
  );
}

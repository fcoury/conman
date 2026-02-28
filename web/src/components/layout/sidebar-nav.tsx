import { NavLink } from "react-router-dom";
import {
  GitBranch,
  GitPullRequest,
  Package,
  Rocket,
  FlaskConical,
  Server,
  Cog,
  LayoutGrid,
  Users,
  Bell,
  Settings,
} from "lucide-react";
import { cn } from "@/lib/utils";

interface NavItem {
  to: string;
  label: string;
  icon: React.ElementType;
}

interface NavGroup {
  label: string;
  items: NavItem[];
}

const navGroups: NavGroup[] = [
  {
    label: "Overview",
    items: [
      { to: "/workspaces", label: "Workspaces", icon: GitBranch },
      { to: "/changesets", label: "Changesets", icon: GitPullRequest },
    ],
  },
  {
    label: "Release",
    items: [
      { to: "/releases", label: "Releases", icon: Package },
      { to: "/deployments", label: "Deployments", icon: Rocket },
      { to: "/temp-envs", label: "Temp Envs", icon: FlaskConical },
    ],
  },
  {
    label: "Operations",
    items: [
      { to: "/runtime", label: "Runtime", icon: Server },
      { to: "/jobs", label: "Jobs", icon: Cog },
    ],
  },
  {
    label: "Management",
    items: [
      { to: "/apps", label: "Apps", icon: LayoutGrid },
      { to: "/members", label: "Members", icon: Users },
      { to: "/notifications", label: "Notifications", icon: Bell },
      { to: "/settings", label: "Settings", icon: Settings },
    ],
  },
];

export function SidebarNav(): React.ReactElement {
  return (
    <nav className="flex flex-col gap-4 px-3 py-2">
      {navGroups.map((group) => (
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
                      ? "bg-sidebar-active/10 text-sidebar-active border-l-[3px] border-sidebar-active -ml-px"
                      : "text-sidebar-foreground hover:bg-accent hover:text-accent-foreground",
                  )
                }
              >
                <item.icon className="h-4 w-4 shrink-0" />
                {item.label}
              </NavLink>
            ))}
          </div>
        </div>
      ))}
    </nav>
  );
}

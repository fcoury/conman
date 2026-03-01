import {
  Sidebar,
  SidebarContent,
  SidebarFooter,
  SidebarHeader,
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem,
} from '@/components/ui/sidebar';
import { FileDiff, FolderCode, LayoutDashboard, Moon, Sun } from 'lucide-react';
import { useTheme } from 'next-themes';
import { Link, useLocation } from 'react-router-dom';

const navItems = [
  { label: 'Dashboard', href: '/', icon: LayoutDashboard },
  { label: 'Workspaces', href: '/workspaces', icon: FolderCode },
  { label: 'My Changes', href: '/changesets', icon: FileDiff },
];

export default function AppSidebar() {
  const { pathname } = useLocation();
  const { resolvedTheme, setTheme } = useTheme();

  return (
    <Sidebar collapsible="icon">
      <SidebarHeader className="p-4">
        <span className="text-lg font-semibold group-data-[collapsible=icon]:hidden">
          Conman
        </span>
      </SidebarHeader>

      <SidebarContent>
        <SidebarMenu>
          {navItems.map((item) => {
            const isActive =
              item.href === '/'
                ? pathname === '/'
                : pathname.startsWith(item.href);

            return (
              <SidebarMenuItem key={item.href}>
                <SidebarMenuButton asChild isActive={isActive} tooltip={item.label}>
                  <Link to={item.href}>
                    <item.icon className="size-4" />
                    <span>{item.label}</span>
                  </Link>
                </SidebarMenuButton>
              </SidebarMenuItem>
            );
          })}
        </SidebarMenu>
      </SidebarContent>

      <SidebarFooter>
        <SidebarMenu>
          <SidebarMenuItem>
            <SidebarMenuButton
              onClick={() =>
                setTheme(resolvedTheme === 'dark' ? 'light' : 'dark')
              }
            >
              {resolvedTheme === 'dark' ? (
                <Sun className="size-4" />
              ) : (
                <Moon className="size-4" />
              )}
              <span>{resolvedTheme === 'dark' ? 'Light mode' : 'Dark mode'}</span>
            </SidebarMenuButton>
          </SidebarMenuItem>
        </SidebarMenu>
      </SidebarFooter>
    </Sidebar>
  );
}

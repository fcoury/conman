import { LogOut } from "lucide-react";
import { useAuth } from "@/hooks/use-auth";
import { DropdownMenu, DropdownMenuItem } from "@/components/ui/dropdown-menu";

export function UserMenu(): React.ReactElement {
  const { logout } = useAuth();

  return (
    <DropdownMenu
      trigger={
        <div className="flex h-8 w-8 items-center justify-center rounded-full bg-primary/15 text-primary text-sm font-medium hover:bg-primary/25 transition-colors">
          U
        </div>
      }
    >
      <DropdownMenuItem onClick={logout} destructive>
        <LogOut className="h-4 w-4" />
        Logout
      </DropdownMenuItem>
    </DropdownMenu>
  );
}

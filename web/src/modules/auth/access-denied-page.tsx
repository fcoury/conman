import { Link } from "react-router-dom";
import { ShieldX } from "lucide-react";
import { Card } from "@/components/ui/card";
import { Logo } from "@/components/ui/logo";

export function AccessDeniedPage({ message }: { message?: string }): React.ReactElement {
  return (
    <div className="flex min-h-screen items-center justify-center bg-background p-4">
      <div className="w-full max-w-lg space-y-6 animate-fade-in-up">
        <div className="flex justify-center">
          <Logo size="lg" />
        </div>
        <Card className="space-y-3">
          <div className="flex items-center gap-2">
            <ShieldX className="h-5 w-5 text-destructive" />
            <h1 className="text-lg font-semibold font-heading">Access denied</h1>
          </div>
          <p className="text-muted-foreground text-sm">
            {message ??
              "Your account does not have membership on the instance currently selected in this console."}
          </p>
          <div className="flex gap-3">
            <Link to="/login" className="text-primary text-sm hover:underline">
              Login with another account
            </Link>
            <Link to="/setup" className="text-primary text-sm hover:underline">
              Open setup
            </Link>
          </div>
        </Card>
      </div>
    </div>
  );
}

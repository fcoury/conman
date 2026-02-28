import { Link } from "react-router-dom";

import { Card } from "@/components/ui/card";

export function AccessDeniedPage({ message }: { message?: string }): React.ReactElement {
  return (
    <div className="from-background to-muted/60 flex min-h-screen items-center justify-center bg-gradient-to-br p-4">
      <Card className="max-w-lg space-y-3">
        <h1 className="text-lg font-semibold">Access denied for bound repo</h1>
        <p className="text-muted-foreground text-sm">
          {message ??
            "Your account does not have membership on the repo currently bound to this Conman instance."}
        </p>
        <div className="flex gap-2">
          <Link to="/login" className="text-primary text-sm underline">
            Login with another account
          </Link>
          <Link to="/setup" className="text-primary text-sm underline">
            Open setup
          </Link>
        </div>
      </Card>
    </div>
  );
}

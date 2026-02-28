import { Link } from "react-router-dom";

import { Card } from "@/components/ui/card";

export function NotFoundPage(): React.ReactElement {
  return (
    <div className="from-background to-muted/60 flex min-h-screen items-center justify-center bg-gradient-to-br p-4">
      <Card className="space-y-2">
        <h1 className="text-lg font-semibold">Page not found</h1>
        <p className="text-muted-foreground text-sm">The requested route does not exist in the Conman UI.</p>
        <Link to="/" className="text-primary text-sm underline">
          Go to dashboard
        </Link>
      </Card>
    </div>
  );
}

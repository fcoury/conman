import { Link } from "react-router-dom";
import { Logo } from "@/components/ui/logo";
import { Card } from "@/components/ui/card";

export function NotFoundPage(): React.ReactElement {
  return (
    <div className="flex min-h-screen items-center justify-center bg-background p-4">
      <div className="w-full max-w-md space-y-6 animate-fade-in-up">
        <div className="flex justify-center">
          <Logo size="lg" />
        </div>
        <Card className="space-y-2">
          <h1 className="text-lg font-semibold font-heading">Page not found</h1>
          <p className="text-muted-foreground text-sm">The requested route does not exist in the Conman UI.</p>
          <Link to="/" className="text-primary text-sm hover:underline">
            Go to dashboard
          </Link>
        </Card>
      </div>
    </div>
  );
}

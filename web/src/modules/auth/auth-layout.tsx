import { Link } from "react-router-dom";
import { Logo } from "@/components/ui/logo";

export function AuthLayout({ title, subtitle, children }: { title: string; subtitle?: string; children: React.ReactNode }): React.ReactElement {
  return (
    <div className="flex min-h-screen items-center justify-center bg-background p-4">
      <div className="w-full max-w-md space-y-6 animate-fade-in-up">
        <div className="flex justify-center">
          <Logo size="lg" />
        </div>
        <div className="bg-card rounded-xl border border-border p-6 shadow-lg shadow-black/10 space-y-4">
          <div>
            <h1 className="text-xl font-semibold font-heading">{title}</h1>
            {subtitle ? <p className="text-muted-foreground mt-1 text-sm">{subtitle}</p> : null}
          </div>
          {children}
        </div>
        <div className="text-muted-foreground flex items-center justify-between text-xs px-1">
          <Link to="/login" className="hover:text-foreground transition-colors">
            Login
          </Link>
          <Link to="/signup" className="hover:text-foreground transition-colors">
            Signup
          </Link>
          <Link to="/forgot-password" className="hover:text-foreground transition-colors">
            Forgot password
          </Link>
        </div>
      </div>
    </div>
  );
}

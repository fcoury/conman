import { Link } from "react-router-dom";

export function AuthLayout({ title, subtitle, children }: { title: string; subtitle?: string; children: React.ReactNode }): React.ReactElement {
  return (
    <div className="from-background to-muted/60 flex min-h-screen items-center justify-center bg-gradient-to-br p-4">
      <div className="bg-card w-full max-w-md space-y-4 rounded-xl border p-6 shadow-sm">
        <div>
          <h1 className="text-xl font-semibold">{title}</h1>
          {subtitle ? <p className="text-muted-foreground mt-1 text-sm">{subtitle}</p> : null}
        </div>
        {children}
        <div className="text-muted-foreground flex items-center justify-between text-xs">
          <Link to="/login" className="hover:text-foreground">
            Login
          </Link>
          <Link to="/signup" className="hover:text-foreground">
            Signup
          </Link>
          <Link to="/forgot-password" className="hover:text-foreground">
            Forgot password
          </Link>
        </div>
      </div>
    </div>
  );
}

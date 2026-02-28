import { FormEvent, useState } from "react";
import { useNavigate } from "react-router-dom";

import { apiData, ApiError } from "@/api/client";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { useAuth } from "@/hooks/use-auth";
import { AuthLayout } from "@/modules/auth/auth-layout";

interface LoginResponse {
  token: string;
  user: { id: string; email: string; name: string };
}

export function LoginPage(): React.ReactElement {
  const navigate = useNavigate();
  const { setToken } = useAuth();
  const [email, setEmail] = useState("admin@example.com");
  const [password, setPassword] = useState("AdminPassw0rd!!");
  const [error, setError] = useState<string | null>(null);
  const [isSubmitting, setIsSubmitting] = useState(false);

  const onSubmit = async (event: FormEvent<HTMLFormElement>): Promise<void> => {
    event.preventDefault();
    setError(null);
    setIsSubmitting(true);
    try {
      const data = await apiData<LoginResponse>("/api/auth/login", {
        method: "POST",
        body: JSON.stringify({ email, password }),
      });
      setToken(data.token);
      navigate("/workspaces", { replace: true });
    } catch (cause) {
      setError(cause instanceof ApiError ? cause.message : "login failed");
    } finally {
      setIsSubmitting(false);
    }
  };

  return (
    <AuthLayout title="Conman Login" subtitle="Sign in with your Conman account.">
      <form className="space-y-3" onSubmit={onSubmit}>
        <Input type="email" value={email} onChange={(event) => setEmail(event.target.value)} placeholder="Email" required />
        <Input
          type="password"
          value={password}
          onChange={(event) => setPassword(event.target.value)}
          placeholder="Password"
          required
        />
        {error ? <p className="text-destructive text-xs">{error}</p> : null}
        <Button type="submit" className="w-full" disabled={isSubmitting}>
          {isSubmitting ? "Signing in..." : "Sign in"}
        </Button>
      </form>
    </AuthLayout>
  );
}

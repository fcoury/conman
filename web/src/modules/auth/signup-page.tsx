import { FormEvent, useState } from "react";
import { useNavigate } from "react-router-dom";

import { apiData, ApiError } from "@/api/client";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { useAuth } from "@/hooks/use-auth";
import { AuthLayout } from "@/modules/auth/auth-layout";

interface SignupResponse {
  token: string;
  user: { id: string; email: string; name: string };
}

export function SignupPage(): React.ReactElement {
  const navigate = useNavigate();
  const { setToken } = useAuth();
  const [name, setName] = useState("Admin User");
  const [email, setEmail] = useState("admin@example.com");
  const [password, setPassword] = useState("AdminPassw0rd!!");
  const [error, setError] = useState<string | null>(null);
  const [isSubmitting, setIsSubmitting] = useState(false);

  const onSubmit = async (event: FormEvent<HTMLFormElement>): Promise<void> => {
    event.preventDefault();
    setError(null);
    setIsSubmitting(true);
    try {
      const data = await apiData<SignupResponse>("/api/auth/signup", {
        method: "POST",
        body: JSON.stringify({ name, email, password }),
      });
      setToken(data.token);
      navigate("/setup", { replace: true });
    } catch (cause) {
      setError(cause instanceof ApiError ? cause.message : "signup failed");
    } finally {
      setIsSubmitting(false);
    }
  };

  return (
    <AuthLayout title="Create account" subtitle="Bootstrap your first Conman team and repository.">
      <form className="space-y-3" onSubmit={onSubmit}>
        <Input value={name} onChange={(event) => setName(event.target.value)} placeholder="Name" required />
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
          {isSubmitting ? "Creating account..." : "Create account"}
        </Button>
      </form>
    </AuthLayout>
  );
}

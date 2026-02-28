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

export function AcceptInvitePage(): React.ReactElement {
  const navigate = useNavigate();
  const { setToken } = useAuth();
  const [token, setInviteToken] = useState("");
  const [name, setName] = useState("");
  const [password, setPassword] = useState("");
  const [error, setError] = useState<string | null>(null);

  const onSubmit = async (event: FormEvent<HTMLFormElement>): Promise<void> => {
    event.preventDefault();
    setError(null);
    try {
      const data = await apiData<LoginResponse>("/api/auth/accept-invite", {
        method: "POST",
        body: JSON.stringify({ token, name, password }),
      });
      setToken(data.token);
      navigate("/workspaces", { replace: true });
    } catch (cause) {
      setError(cause instanceof ApiError ? cause.message : "invite acceptance failed");
    }
  };

  return (
    <AuthLayout title="Accept invite" subtitle="Join a Conman team using invite token.">
      <form className="space-y-3" onSubmit={onSubmit}>
        <Input value={token} onChange={(event) => setInviteToken(event.target.value)} placeholder="Invite token" required />
        <Input value={name} onChange={(event) => setName(event.target.value)} placeholder="Full name" required />
        <Input
          type="password"
          value={password}
          onChange={(event) => setPassword(event.target.value)}
          placeholder="Password"
          required
        />
        {error ? <p className="text-destructive text-xs">{error}</p> : null}
        <Button type="submit" className="w-full">
          Accept invite
        </Button>
      </form>
    </AuthLayout>
  );
}

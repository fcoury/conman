import { FormEvent, useState } from "react";

import { apiData, ApiError } from "@/api/client";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { AuthLayout } from "@/modules/auth/auth-layout";

interface MessageResponse {
  message: string;
}

export function ResetPasswordPage(): React.ReactElement {
  const [token, setToken] = useState("");
  const [newPassword, setNewPassword] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [message, setMessage] = useState<string | null>(null);

  const onSubmit = async (event: FormEvent<HTMLFormElement>): Promise<void> => {
    event.preventDefault();
    setError(null);
    try {
      const data = await apiData<MessageResponse>("/api/auth/reset-password", {
        method: "POST",
        body: JSON.stringify({ token, new_password: newPassword }),
      });
      setMessage(data.message);
    } catch (cause) {
      setError(cause instanceof ApiError ? cause.message : "reset failed");
    }
  };

  return (
    <AuthLayout title="Apply reset token" subtitle="Set a new password using your reset token.">
      <form className="space-y-3" onSubmit={onSubmit}>
        <Input value={token} onChange={(event) => setToken(event.target.value)} placeholder="Reset token" required />
        <Input
          type="password"
          value={newPassword}
          onChange={(event) => setNewPassword(event.target.value)}
          placeholder="New password"
          required
        />
        {error ? <p className="text-destructive text-xs">{error}</p> : null}
        {message ? <p className="text-success-foreground text-xs">{message}</p> : null}
        <Button type="submit" className="w-full">
          Update password
        </Button>
      </form>
    </AuthLayout>
  );
}

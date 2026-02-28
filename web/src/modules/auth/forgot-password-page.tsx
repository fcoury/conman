import { FormEvent, useState } from "react";

import { apiData, ApiError } from "@/api/client";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { AuthLayout } from "@/modules/auth/auth-layout";

interface ForgotResponse {
  message: string;
  reset_token?: string;
}

export function ForgotPasswordPage(): React.ReactElement {
  const [email, setEmail] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [message, setMessage] = useState<string | null>(null);
  const [tokenPreview, setTokenPreview] = useState<string | null>(null);

  const onSubmit = async (event: FormEvent<HTMLFormElement>): Promise<void> => {
    event.preventDefault();
    setError(null);
    try {
      const data = await apiData<ForgotResponse>("/api/auth/forgot-password", {
        method: "POST",
        body: JSON.stringify({ email }),
      });
      setMessage(data.message);
      setTokenPreview(data.reset_token ?? null);
    } catch (cause) {
      setError(cause instanceof ApiError ? cause.message : "forgot password failed");
    }
  };

  return (
    <AuthLayout title="Reset password" subtitle="Request a reset token for your account.">
      <form className="space-y-3" onSubmit={onSubmit}>
        <Input type="email" value={email} onChange={(event) => setEmail(event.target.value)} placeholder="Email" required />
        {error ? <p className="text-destructive text-xs">{error}</p> : null}
        {message ? <p className="text-success-foreground text-xs">{message}</p> : null}
        {tokenPreview ? (
          <div className="bg-muted rounded-md p-2 text-xs">
            Reset token (dev/testing): <code>{tokenPreview}</code>
          </div>
        ) : null}
        <Button type="submit" className="w-full">
          Request token
        </Button>
      </form>
    </AuthLayout>
  );
}

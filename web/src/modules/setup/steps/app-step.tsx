import { FormEvent, useState } from "react";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { useApi } from "@/hooks/use-api";
import { ApiError } from "@/api/client";

interface AppStepProps {
  repoId: string;
  onNext: () => void;
  onBack: () => void;
}

export function AppStep({ repoId, onNext, onBack }: AppStepProps): React.ReactElement {
  const api = useApi();
  const [key, setKey] = useState("portal");
  const [title, setTitle] = useState("Primary App");
  const [domain, setDomain] = useState("portal.example.test");
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);

  const handleCreate = async (e: FormEvent) => {
    e.preventDefault();
    setError(null);
    setLoading(true);
    try {
      await api.data(`/api/repos/${repoId}/apps`, {
        method: "POST",
        body: JSON.stringify({
          key,
          title,
          domains: domain ? [domain] : [],
        }),
      });
      onNext();
    } catch (cause) {
      setError(cause instanceof ApiError ? cause.message : "Failed to create app");
    } finally {
      setLoading(false);
    }
  };

  return (
    <div className="space-y-4">
      <div>
        <h2 className="text-lg font-semibold font-heading">First App</h2>
        <p className="text-sm text-muted-foreground">Create your first application, or skip this step.</p>
      </div>

      {error && (
        <div className="rounded-md bg-destructive/10 border border-destructive/30 px-3 py-2 text-sm text-destructive">
          {error}
        </div>
      )}

      <Card>
        <form className="space-y-3" onSubmit={(e) => void handleCreate(e)}>
          <Input label="App key" id="app-key" value={key} onChange={(e) => setKey(e.target.value)} required />
          <Input label="Title" id="app-title" value={title} onChange={(e) => setTitle(e.target.value)} required />
          <Input label="Domain" id="app-domain" value={domain} onChange={(e) => setDomain(e.target.value)} placeholder="optional" />
          <div className="flex justify-between">
            <Button variant="ghost" onClick={onBack} type="button">Back</Button>
            <div className="flex gap-2">
              <Button variant="secondary" onClick={onNext} type="button">Skip</Button>
              <Button type="submit" disabled={loading}>
                {loading ? "Creating..." : "Create & continue"}
              </Button>
            </div>
          </div>
        </form>
      </Card>
    </div>
  );
}

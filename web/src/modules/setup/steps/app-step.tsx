import { FormEvent, useMemo, useState } from "react";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { useApi } from "@/hooks/use-api";
import { ApiError } from "@/api/client";

interface AppStepProps {
  repoId: string;
  instanceSlug: string;
  onNext: () => void;
}

function slugify(value: string): string {
  return value
    .toLowerCase()
    .trim()
    .replace(/[^a-z0-9-_]+/g, "-")
    .replace(/^-+|-+$/g, "");
}

export function AppStep({ repoId, instanceSlug, onNext }: AppStepProps): React.ReactElement {
  const api = useApi();
  const [key, setKey] = useState("portal");
  const [title, setTitle] = useState("Primary App");
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);

  const hostnamePreview = useMemo(() => {
    const effectiveKey = slugify(key) || "app";
    return `${effectiveKey}--${instanceSlug}.dxflow-app.com`;
  }, [key, instanceSlug]);

  const handleCreate = async (e: FormEvent) => {
    e.preventDefault();
    setError(null);
    const normalizedKey = slugify(key);
    if (!normalizedKey) {
      setError("App key is required.");
      return;
    }
    setLoading(true);
    try {
      await api.data(`/api/repos/${repoId}/apps`, {
        method: "POST",
        body: JSON.stringify({
          key: normalizedKey,
          title,
          domains: [hostnamePreview],
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
        <p className="text-sm text-muted-foreground">
          Create your first app for this instance, or skip this step.
        </p>
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
          <p className="text-xs text-muted-foreground">
            URL format: <span className="font-medium text-foreground">{hostnamePreview}</span>
          </p>
          <div className="flex justify-between">
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

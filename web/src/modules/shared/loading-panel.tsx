import { LoaderCircle } from "lucide-react";

import { Card } from "@/components/ui/card";

export function LoadingPanel({ label }: { label?: string }): React.ReactElement {
  return (
    <Card className="flex items-center gap-2 p-4">
      <LoaderCircle className="text-muted-foreground h-4 w-4 animate-spin" />
      <span className="text-muted-foreground text-sm">{label ?? "Loading..."}</span>
    </Card>
  );
}

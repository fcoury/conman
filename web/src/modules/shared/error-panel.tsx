import { AlertTriangle } from "lucide-react";

import { Card } from "@/components/ui/card";

export function ErrorPanel({ title, detail }: { title: string; detail?: string }): React.ReactElement {
  return (
    <Card className="border-destructive/35 bg-destructive/5">
      <div className="flex items-start gap-2">
        <AlertTriangle className="text-destructive mt-0.5 h-4 w-4" />
        <div>
          <p className="text-sm font-semibold">{title}</p>
          {detail ? <p className="text-muted-foreground text-xs">{detail}</p> : null}
        </div>
      </div>
    </Card>
  );
}

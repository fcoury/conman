import { AlertTriangle } from "lucide-react";

export function ErrorPanel({ title, detail }: { title: string; detail?: string }): React.ReactElement {
  return (
    <div className="rounded-lg border border-destructive/30 bg-destructive/10 p-4">
      <div className="flex items-start gap-2">
        <AlertTriangle className="text-destructive mt-0.5 h-4 w-4 shrink-0" />
        <div>
          <p className="text-sm font-semibold text-foreground">{title}</p>
          {detail ? <p className="text-muted-foreground text-xs mt-0.5">{detail}</p> : null}
        </div>
      </div>
    </div>
  );
}

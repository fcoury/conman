import { LoaderCircle } from "lucide-react";
import { Skeleton } from "@/components/ui/skeleton";

export function LoadingPanel({ label }: { label?: string }): React.ReactElement {
  return (
    <div className="flex items-center gap-2 p-4">
      <LoaderCircle className="text-muted-foreground h-4 w-4 animate-spin" />
      <span className="text-muted-foreground text-sm">{label ?? "Loading..."}</span>
    </div>
  );
}

export function LoadingSkeleton({ rows = 3 }: { rows?: number }): React.ReactElement {
  return (
    <div className="space-y-3">
      {Array.from({ length: rows }).map((_, i) => (
        <Skeleton key={i} className="h-16 w-full rounded-xl" />
      ))}
    </div>
  );
}

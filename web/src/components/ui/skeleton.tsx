import { cn } from "@/lib/utils";

export function Skeleton({ className }: { className?: string }): React.ReactElement {
  return <div className={cn("animate-pulse rounded-md bg-muted", className)} />;
}

export function SkeletonText({ lines = 3, className }: { lines?: number; className?: string }): React.ReactElement {
  return (
    <div className={cn("space-y-2", className)}>
      {Array.from({ length: lines }).map((_, i) => (
        <Skeleton key={i} className={cn("h-4", i === lines - 1 ? "w-3/4" : "w-full")} />
      ))}
    </div>
  );
}

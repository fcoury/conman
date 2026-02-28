import { cn } from "@/lib/utils";

function colorForState(value: string): string {
  const normalized = value.toLowerCase();
  if (normalized.includes("succeed") || normalized.includes("active") || normalized.includes("approved")) {
    return "bg-success/20 text-success-foreground";
  }
  if (normalized.includes("fail") || normalized.includes("reject") || normalized.includes("conflict")) {
    return "bg-destructive/20 text-destructive";
  }
  if (normalized.includes("running") || normalized.includes("queue") || normalized.includes("review")) {
    return "bg-indigo/20 text-indigo-foreground";
  }
  return "bg-muted text-muted-foreground";
}

export function StatusPill({ label }: { label: string }): React.ReactElement {
  return <span className={cn("inline-flex rounded-full px-2 py-0.5 text-xs font-medium", colorForState(label))}>{label}</span>;
}

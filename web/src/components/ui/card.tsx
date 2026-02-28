import { cn } from "@/lib/utils";

export function Card({ className, children }: { className?: string; children: React.ReactNode }): React.ReactElement {
  return <div className={cn("bg-card text-card-foreground rounded-xl border p-4", className)}>{children}</div>;
}

export function CardTitle({ className, children }: { className?: string; children: React.ReactNode }): React.ReactElement {
  return <h2 className={cn("text-base font-semibold", className)}>{children}</h2>;
}

export function CardDescription({ children }: { children: React.ReactNode }): React.ReactElement {
  return <p className="text-muted-foreground text-sm">{children}</p>;
}

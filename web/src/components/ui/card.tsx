import { cn } from "@/lib/utils";

export function Card({
  className,
  elevation = "default",
  children,
}: {
  className?: string;
  elevation?: "flat" | "default" | "raised";
  children: React.ReactNode;
}): React.ReactElement {
  return (
    <div
      className={cn(
        "bg-card text-card-foreground rounded-xl border p-4",
        elevation === "flat" && "border-transparent bg-transparent",
        elevation === "raised" && "shadow-lg shadow-black/20",
        className,
      )}
    >
      {children}
    </div>
  );
}

export function CardHeader({ className, children }: { className?: string; children: React.ReactNode }): React.ReactElement {
  return <div className={cn("flex flex-col gap-1.5 pb-3", className)}>{children}</div>;
}

export function CardContent({ className, children }: { className?: string; children: React.ReactNode }): React.ReactElement {
  return <div className={cn("", className)}>{children}</div>;
}

export function CardFooter({ className, children }: { className?: string; children: React.ReactNode }): React.ReactElement {
  return <div className={cn("flex items-center gap-2 pt-3 border-t border-border", className)}>{children}</div>;
}

export function CardTitle({ className, children }: { className?: string; children: React.ReactNode }): React.ReactElement {
  return <h2 className={cn("text-base font-semibold font-heading", className)}>{children}</h2>;
}

export function CardDescription({ children }: { children: React.ReactNode }): React.ReactElement {
  return <p className="text-muted-foreground text-sm">{children}</p>;
}

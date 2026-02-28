import type { SelectHTMLAttributes } from "react";
import { cn } from "@/lib/utils";

export function Select({ className, ...props }: SelectHTMLAttributes<HTMLSelectElement>): React.ReactElement {
  return (
    <select
      className={cn(
        "border-border bg-input text-foreground w-full rounded-md border px-3 py-2 text-sm",
        "focus:border-primary focus:ring-2 focus:ring-primary/20 focus:outline-none",
        "transition-colors duration-150",
        className,
      )}
      {...props}
    />
  );
}

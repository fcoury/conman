import type { SelectHTMLAttributes } from "react";
import { cn } from "@/lib/utils";

interface SelectProps extends SelectHTMLAttributes<HTMLSelectElement> {
  label?: string;
}

export function Select({ className, label, id, ...props }: SelectProps): React.ReactElement {
  if (label) {
    return (
      <div className="space-y-1.5">
        <label htmlFor={id} className="text-sm font-medium text-foreground">
          {label}
        </label>
        <select
          id={id}
          className={cn(
            "border-border bg-input text-foreground w-full rounded-md border px-3 py-2 text-sm",
            "focus:border-primary focus:ring-2 focus:ring-primary/20 focus:outline-none",
            "transition-colors duration-150",
            className,
          )}
          {...props}
        />
      </div>
    );
  }

  return (
    <select
      id={id}
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

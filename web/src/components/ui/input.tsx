import type { InputHTMLAttributes } from "react";
import { cn } from "@/lib/utils";

interface InputProps extends InputHTMLAttributes<HTMLInputElement> {
  label?: string;
}

export function Input({ className, label, id, ...props }: InputProps): React.ReactElement {
  if (label) {
    return (
      <div className="space-y-1.5">
        <label htmlFor={id} className="text-sm font-medium text-foreground">
          {label}
        </label>
        <input
          id={id}
          className={cn(
            "border-border bg-input text-foreground placeholder:text-muted-foreground w-full rounded-md border px-3 py-2 text-sm",
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
    <input
      id={id}
      className={cn(
        "border-border bg-input text-foreground placeholder:text-muted-foreground w-full rounded-md border px-3 py-2 text-sm",
        "focus:border-primary focus:ring-2 focus:ring-primary/20 focus:outline-none",
        "transition-colors duration-150",
        className,
      )}
      {...props}
    />
  );
}

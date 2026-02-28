import type { TextareaHTMLAttributes } from "react";
import { cn } from "@/lib/utils";

interface TextareaProps extends TextareaHTMLAttributes<HTMLTextAreaElement> {
  label?: string;
}

export function Textarea({ className, label, id, ...props }: TextareaProps): React.ReactElement {
  if (label) {
    return (
      <div className="space-y-1.5">
        <label htmlFor={id} className="text-sm font-medium text-foreground">
          {label}
        </label>
        <textarea
          id={id}
          className={cn(
            "border-border bg-input text-foreground placeholder:text-muted-foreground min-h-24 w-full rounded-md border px-3 py-2 text-sm",
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
    <textarea
      id={id}
      className={cn(
        "border-border bg-input text-foreground placeholder:text-muted-foreground min-h-24 w-full rounded-md border px-3 py-2 text-sm",
        "focus:border-primary focus:ring-2 focus:ring-primary/20 focus:outline-none",
        "transition-colors duration-150",
        className,
      )}
      {...props}
    />
  );
}

import type { TextareaHTMLAttributes } from "react";
import { cn } from "@/lib/utils";

export function Textarea({ className, ...props }: TextareaHTMLAttributes<HTMLTextAreaElement>): React.ReactElement {
  return (
    <textarea
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

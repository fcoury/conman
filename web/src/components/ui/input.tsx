import type { InputHTMLAttributes } from "react";

import { cn } from "@/lib/utils";

export function Input({ className, ...props }: InputHTMLAttributes<HTMLInputElement>): React.ReactElement {
  return (
    <input
      className={cn(
        "border-input bg-background text-foreground focus-visible:ring-ring w-full rounded-md border px-3 py-2 text-sm",
        "focus-visible:outline-none focus-visible:ring-2",
        className,
      )}
      {...props}
    />
  );
}

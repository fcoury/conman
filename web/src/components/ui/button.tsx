import type { ButtonHTMLAttributes } from "react";

import { cn } from "@/lib/utils";

type Variant = "primary" | "secondary" | "danger" | "ghost";

const variantClassMap: Record<Variant, string> = {
  primary: "bg-primary text-primary-foreground hover:bg-primary/90",
  secondary: "bg-secondary text-secondary-foreground hover:bg-secondary/85",
  danger: "bg-destructive text-white hover:bg-destructive/90",
  ghost: "bg-transparent text-foreground hover:bg-accent hover:text-accent-foreground",
};

export interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: Variant;
}

export function Button({ className, variant = "primary", ...props }: ButtonProps): React.ReactElement {
  return (
    <button
      className={cn(
        "inline-flex cursor-pointer items-center justify-center gap-2 rounded-md px-3 py-2 text-sm font-medium",
        "disabled:cursor-not-allowed disabled:opacity-50",
        variantClassMap[variant],
        className,
      )}
      {...props}
    />
  );
}

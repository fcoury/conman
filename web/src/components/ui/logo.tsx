import { cn } from "@/lib/utils";

export function Logo({ className, size = "md" }: { className?: string; size?: "sm" | "md" | "lg" }): React.ReactElement {
  const sizes = { sm: "h-6", md: "h-8", lg: "h-10" };
  const textSizes = { sm: "text-sm", md: "text-lg", lg: "text-xl" };

  return (
    <div className={cn("flex items-center gap-2", className)}>
      <svg viewBox="0 0 24 24" fill="none" className={cn(sizes[size], "w-auto")} aria-hidden>
        {/* Config sliders mark */}
        <rect x="3" y="4" width="18" height="2" rx="1" fill="currentColor" opacity="0.3" />
        <rect x="3" y="11" width="18" height="2" rx="1" fill="currentColor" opacity="0.3" />
        <rect x="3" y="18" width="18" height="2" rx="1" fill="currentColor" opacity="0.3" />
        <circle cx="8" cy="5" r="2.5" fill="oklch(0.75 0.16 70)" />
        <circle cx="16" cy="12" r="2.5" fill="oklch(0.75 0.16 70)" />
        <circle cx="11" cy="19" r="2.5" fill="oklch(0.75 0.16 70)" />
      </svg>
      <span className={cn("font-heading font-bold tracking-tight text-foreground", textSizes[size])}>
        conman
      </span>
    </div>
  );
}

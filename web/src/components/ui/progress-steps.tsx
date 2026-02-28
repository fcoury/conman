import { cn } from "@/lib/utils";
import { Check } from "lucide-react";

interface ProgressStepsProps {
  steps: string[];
  currentStep: number;
}

export function ProgressSteps({ steps, currentStep }: ProgressStepsProps): React.ReactElement {
  return (
    <div className="flex items-center gap-2">
      {steps.map((label, i) => {
        const isComplete = i < currentStep;
        const isCurrent = i === currentStep;

        return (
          <div key={label} className="flex items-center gap-2">
            {i > 0 && (
              <div className={cn("h-px w-6", isComplete ? "bg-primary" : "bg-border")} />
            )}
            <div className="flex items-center gap-1.5">
              <div
                className={cn(
                  "flex h-6 w-6 items-center justify-center rounded-full text-xs font-medium transition-colors",
                  isComplete && "bg-primary text-primary-foreground",
                  isCurrent && "bg-primary text-primary-foreground ring-2 ring-primary/30",
                  !isComplete && !isCurrent && "bg-muted text-muted-foreground",
                )}
              >
                {isComplete ? <Check className="h-3.5 w-3.5" /> : i + 1}
              </div>
              <span
                className={cn(
                  "hidden text-xs sm:inline",
                  isCurrent ? "text-foreground font-medium" : "text-muted-foreground",
                )}
              >
                {label}
              </span>
            </div>
          </div>
        );
      })}
    </div>
  );
}

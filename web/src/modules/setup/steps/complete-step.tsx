import { CheckCircle } from "lucide-react";
import { Button } from "@/components/ui/button";

interface CompleteStepProps {
  error?: string | null;
  onGoToDashboard: () => void;
}

export function CompleteStep({ error, onGoToDashboard }: CompleteStepProps): React.ReactElement {
  return (
    <div className="space-y-6 text-center">
      <div className="flex justify-center">
        <div className="animate-scale-check flex h-16 w-16 items-center justify-center rounded-full bg-success/20 text-success-foreground">
          <CheckCircle className="h-8 w-8" />
        </div>
      </div>
      <div>
        <h2 className="text-2xl font-semibold font-heading">You're all set!</h2>
        <p className="text-sm text-muted-foreground mt-2">
          Your Conman instance is configured and ready to use.
        </p>
      </div>
      <Button onClick={onGoToDashboard} size="lg" className="mx-auto">
        Go to Dashboard
      </Button>
      {error ? (
        <p className="text-sm text-destructive">{error}</p>
      ) : null}
    </div>
  );
}

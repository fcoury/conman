import { Logo } from "@/components/ui/logo";
import { ProgressSteps } from "@/components/ui/progress-steps";

interface WizardLayoutProps {
  currentStep: number;
  steps: string[];
  children: React.ReactNode;
}

export function WizardLayout({ currentStep, steps, children }: WizardLayoutProps): React.ReactElement {
  return (
    <div className="flex min-h-screen flex-col items-center justify-center bg-background px-4 py-12">
      <div className="w-full max-w-lg space-y-8">
        <div className="flex flex-col items-center gap-6">
          <Logo size="lg" />
          <ProgressSteps steps={steps} currentStep={currentStep} />
        </div>
        <div className="animate-slide-in-right">
          {children}
        </div>
      </div>
    </div>
  );
}

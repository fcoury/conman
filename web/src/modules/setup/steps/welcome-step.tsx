import { Rocket, Link2 } from "lucide-react";

interface WelcomeStepProps {
  onNewProject: () => void;
  onBindExisting: () => void;
}

export function WelcomeStep({ onNewProject, onBindExisting }: WelcomeStepProps): React.ReactElement {
  return (
    <div className="space-y-4 text-center">
      <h2 className="text-2xl font-semibold font-heading">Welcome to Conman</h2>
      <p className="text-muted-foreground text-sm">
        Set up your configuration management workspace. Choose how to get started.
      </p>
      <div className="grid gap-3 pt-2">
        <button
          type="button"
          onClick={onNewProject}
          className="cursor-pointer rounded-xl border border-border bg-card p-5 text-left transition-colors hover:border-primary/40"
        >
          <div className="flex items-center gap-3">
            <div className="flex h-10 w-10 items-center justify-center rounded-lg bg-primary/15 text-primary">
              <Rocket className="h-5 w-5" />
            </div>
            <div>
              <p className="font-medium text-sm text-card-foreground">New project</p>
              <p className="text-muted-foreground text-xs">Create a team, repository, and first app</p>
            </div>
          </div>
        </button>
        <button
          type="button"
          onClick={onBindExisting}
          className="cursor-pointer rounded-xl border border-border bg-card p-5 text-left transition-colors hover:border-primary/40"
        >
          <div className="flex items-center gap-3">
            <div className="flex h-10 w-10 items-center justify-center rounded-lg bg-secondary text-secondary-foreground">
              <Link2 className="h-5 w-5" />
            </div>
            <div>
              <p className="font-medium text-sm text-card-foreground">Bind existing</p>
              <p className="text-muted-foreground text-xs">Connect to a repository that's already set up</p>
            </div>
          </div>
        </button>
      </div>
    </div>
  );
}

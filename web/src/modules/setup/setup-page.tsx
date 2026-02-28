import { Navigate } from "react-router-dom";
import { useRepoContextQuery } from "@/hooks/use-repo-context";
import { LoadingPanel } from "@/modules/shared/loading-panel";
import { SetupWizard } from "./setup-wizard";

export function SetupPage(): React.ReactElement {
  const contextQuery = useRepoContextQuery();

  if (contextQuery.isLoading) {
    return (
      <div className="flex h-screen items-center justify-center bg-background">
        <LoadingPanel label="Checking configuration..." />
      </div>
    );
  }

  // Already bound — setup is complete, continue to primary workspace view
  if (contextQuery.data?.status === "bound") {
    return <Navigate to="/workspaces" replace />;
  }

  return <SetupWizard />;
}

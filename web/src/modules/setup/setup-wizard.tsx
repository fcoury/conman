import { useEffect, useState } from "react";
import { useNavigate } from "react-router-dom";
import { useQueryClient } from "@tanstack/react-query";

import { ApiError } from "@/api/client";
import { useApi } from "@/hooks/use-api";
import { useAuth } from "@/hooks/use-auth";
import { useRepoContextQuery } from "@/hooks/use-repo-context";
import { WizardLayout } from "./wizard-layout";
import { InstanceStep } from "./steps/instance-step";
import { AppStep } from "./steps/app-step";
import { CompleteStep } from "./steps/complete-step";

export function SetupWizard(): React.ReactElement {
  const api = useApi();
  const navigate = useNavigate();
  const { setToken } = useAuth();
  const contextQuery = useRepoContextQuery();
  const queryClient = useQueryClient();
  const [currentStep, setCurrentStep] = useState(0);
  const [instanceRepoId, setInstanceRepoId] = useState("");
  const [instanceSlug, setInstanceSlug] = useState("");
  const [finalizeError, setFinalizeError] = useState<string | null>(null);

  const goToDashboard = async () => {
    if (!instanceRepoId) return;
    setFinalizeError(null);
    await api.data("/api/repo", {
      method: "PATCH",
      body: JSON.stringify({ repo_id: instanceRepoId }),
    });
    await queryClient.invalidateQueries({ queryKey: ["repo-context"] });
    navigate("/workspaces", { replace: true });
  };

  const handleInstanceCreated = (payload: {
    token: string;
    repoId: string;
    instanceSlug: string;
  }) => {
    setToken(payload.token);
    setInstanceRepoId(payload.repoId);
    setInstanceSlug(payload.instanceSlug);
    setFinalizeError(null);
    setCurrentStep(1);
  };

  useEffect(() => {
    if (
      currentStep === 0 &&
      !instanceRepoId &&
      contextQuery.data?.status === "bound"
    ) {
      navigate("/workspaces", { replace: true });
    }
  }, [contextQuery.data?.status, currentStep, instanceRepoId, navigate]);

  return (
    <WizardLayout
      currentStep={currentStep}
      steps={["Instance", "First App", "Complete"]}
    >
      {currentStep === 0 && (
        <InstanceStep onCreated={handleInstanceCreated} />
      )}
      {currentStep === 1 && (
        <AppStep
          repoId={instanceRepoId}
          instanceSlug={instanceSlug}
          onNext={() => setCurrentStep(2)}
        />
      )}
      {currentStep === 2 && (
        <CompleteStep
          error={finalizeError}
          onGoToDashboard={() => {
            void goToDashboard().catch((cause) => {
              setFinalizeError(cause instanceof ApiError ? cause.message : "Failed to finish setup");
            });
          }}
        />
      )}
    </WizardLayout>
  );
}

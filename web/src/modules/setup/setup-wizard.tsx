import { useState } from "react";
import { useNavigate } from "react-router-dom";
import { useQueryClient } from "@tanstack/react-query";

import { WizardLayout } from "./wizard-layout";
import { WelcomeStep } from "./steps/welcome-step";
import { TeamStep } from "./steps/team-step";
import { RepoStep } from "./steps/repo-step";
import { AppStep } from "./steps/app-step";
import { BindStep } from "./steps/bind-step";
import { CompleteStep } from "./steps/complete-step";

export function SetupWizard(): React.ReactElement {
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const [currentStep, setCurrentStep] = useState(0);
  const [selectedTeamId, setSelectedTeamId] = useState("");
  const [selectedRepoId, setSelectedRepoId] = useState("");
  // "bind existing" path skips directly to the bind step
  const [fastTrack, setFastTrack] = useState(false);

  const goToDashboard = async () => {
    await queryClient.invalidateQueries({ queryKey: ["repo-context"] });
    navigate("/workspaces", { replace: true });
  };

  const handleBound = () => {
    setCurrentStep(5);
  };

  return (
    <WizardLayout currentStep={currentStep}>
      {currentStep === 0 && (
        <WelcomeStep
          onNewProject={() => setCurrentStep(1)}
          onBindExisting={() => {
            setFastTrack(true);
            setCurrentStep(4);
          }}
        />
      )}
      {currentStep === 1 && (
        <TeamStep
          selectedTeamId={selectedTeamId}
          onSelect={setSelectedTeamId}
          onNext={() => setCurrentStep(2)}
          onBack={() => setCurrentStep(0)}
        />
      )}
      {currentStep === 2 && (
        <RepoStep
          teamId={selectedTeamId}
          selectedRepoId={selectedRepoId}
          onSelect={setSelectedRepoId}
          onNext={() => setCurrentStep(3)}
          onBack={() => setCurrentStep(1)}
        />
      )}
      {currentStep === 3 && (
        <AppStep
          repoId={selectedRepoId}
          onNext={() => setCurrentStep(4)}
          onBack={() => setCurrentStep(2)}
        />
      )}
      {currentStep === 4 && (
        <BindStep
          selectedRepoId={selectedRepoId}
          onSelect={setSelectedRepoId}
          onBind={handleBound}
          onBack={() => setCurrentStep(fastTrack ? 0 : 3)}
        />
      )}
      {currentStep === 5 && (
        <CompleteStep onGoToDashboard={() => void goToDashboard()} />
      )}
    </WizardLayout>
  );
}

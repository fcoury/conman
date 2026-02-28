import type { Deployment } from "@/types/api";

export type DeploymentHistoryFilterState = "all" | "running" | "succeeded" | "failed";

export function classifyDeploymentState(state: string): Exclude<DeploymentHistoryFilterState, "all"> {
  const normalized = state.toLowerCase();
  if (normalized.includes("run") || normalized.includes("queue") || normalized.includes("progress")) {
    return "running";
  }
  if (normalized.includes("success") || normalized.includes("succeed") || normalized.includes("active")) {
    return "succeeded";
  }
  return "failed";
}

export function countDeploymentHistoryByState(deployments: Deployment[]): Record<DeploymentHistoryFilterState, number> {
  return {
    all: deployments.length,
    running: deployments.filter((deployment) => classifyDeploymentState(deployment.state) === "running").length,
    succeeded: deployments.filter((deployment) => classifyDeploymentState(deployment.state) === "succeeded").length,
    failed: deployments.filter((deployment) => classifyDeploymentState(deployment.state) === "failed").length,
  };
}

export function filterDeploymentHistory(
  deployments: Deployment[],
  environmentFilter: string,
  stateFilter: DeploymentHistoryFilterState,
  searchTerm: string,
): Deployment[] {
  const sorted = [...deployments].sort((a, b) => b.created_at.localeCompare(a.created_at));
  const normalizedSearch = searchTerm.trim().toLowerCase();

  return sorted.filter((deployment) => {
    if (environmentFilter !== "all" && deployment.environment_id !== environmentFilter) {
      return false;
    }
    if (stateFilter !== "all" && classifyDeploymentState(deployment.state) !== stateFilter) {
      return false;
    }
    if (!normalizedSearch) {
      return true;
    }

    const haystack = [deployment.id, deployment.release_id, deployment.environment_id, deployment.state]
      .join(" ")
      .toLowerCase();
    return haystack.includes(normalizedSearch);
  });
}

import { createContext, useContext } from "react";
import { useQuery } from "@tanstack/react-query";

import { apiData } from "@/api/client";
import { useAuth } from "@/hooks/use-auth";
import type { RepoContextResponse } from "@/types/api";

const RepoContextData = createContext<RepoContextResponse | null>(null);

export function RepoContextProvider({
  value,
  children,
}: {
  value: RepoContextResponse | null;
  children: React.ReactNode;
}): React.ReactElement {
  return <RepoContextData.Provider value={value}>{children}</RepoContextData.Provider>;
}

export function useRepoContext(): RepoContextResponse | null {
  return useContext(RepoContextData);
}

export function useRepoContextQuery() {
  const { token } = useAuth();

  return useQuery({
    queryKey: ["repo-context", token],
    queryFn: () => apiData<RepoContextResponse>("/api/repo", { method: "GET" }, token ?? undefined),
    enabled: Boolean(token),
    staleTime: 10_000,
  });
}

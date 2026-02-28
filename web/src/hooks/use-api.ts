import { useCallback } from "react";

import { apiData, apiPaginated, apiRequest, ApiError } from "@/api/client";
import { useAuth } from "@/hooks/use-auth";

export function useApi() {
  const { token, logout } = useAuth();

  const wrap = useCallback(
    async <T>(callback: () => Promise<T>): Promise<T> => {
      try {
        return await callback();
      } catch (error) {
        if (error instanceof ApiError && error.status === 401) {
          logout();
        }
        throw error;
      }
    },
    [logout],
  );

  return {
    token,
    request: <T>(path: string, init: RequestInit = {}) => wrap(() => apiRequest<T>(path, init, token ?? undefined)),
    data: <T>(path: string, init: RequestInit = {}) => wrap(() => apiData<T>(path, init, token ?? undefined)),
    paginated: <T>(path: string, init: RequestInit = {}) =>
      wrap(() => apiPaginated<T>(path, init, token ?? undefined)),
  };
}

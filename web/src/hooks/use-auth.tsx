import { createContext, useContext, useMemo, useState } from "react";

const STORAGE_KEY = "conman.auth.token";

interface AuthContextValue {
  token: string | null;
  isAuthenticated: boolean;
  setToken: (token: string | null) => void;
  logout: () => void;
}

const AuthContext = createContext<AuthContextValue | null>(null);

function readStoredToken(): string | null {
  const value = localStorage.getItem(STORAGE_KEY);
  return value && value.trim() ? value : null;
}

export function AuthProvider({ children }: { children: React.ReactNode }): React.ReactElement {
  const [token, setTokenState] = useState<string | null>(() => readStoredToken());

  const setToken = (nextToken: string | null): void => {
    if (nextToken) {
      localStorage.setItem(STORAGE_KEY, nextToken);
      setTokenState(nextToken);
      return;
    }
    localStorage.removeItem(STORAGE_KEY);
    setTokenState(null);
  };

  const value = useMemo<AuthContextValue>(
    () => ({
      token,
      isAuthenticated: Boolean(token),
      setToken,
      logout: () => setToken(null),
    }),
    [token],
  );

  return <AuthContext.Provider value={value}>{children}</AuthContext.Provider>;
}

export function useAuth(): AuthContextValue {
  const context = useContext(AuthContext);
  if (!context) {
    throw new Error("useAuth must be used within AuthProvider");
  }
  return context;
}

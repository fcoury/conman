import { createContext, useContext, useMemo, useState, type ReactNode } from 'react';

import { logout as logoutRequest, login as loginRequest, signup as signupRequest } from './auth-api';
import { clearSession, clearTeamSelection, readSession, writeSession } from './auth-storage';
import type { AuthSession, LoginInput, SignupInput } from './auth-types';

interface AuthContextValue {
  session: AuthSession | null;
  isAuthenticated: boolean;
  login: (input: LoginInput) => Promise<void>;
  signup: (input: SignupInput) => Promise<void>;
  logout: () => Promise<void>;
}

const AuthContext = createContext<AuthContextValue | undefined>(undefined);

export function AuthProvider({ children }: { children: ReactNode }) {
  const [session, setSession] = useState<AuthSession | null>(() => readSession());

  const value = useMemo<AuthContextValue>(() => {
    return {
      session,
      isAuthenticated: session !== null,
      async login(input) {
        const response = await loginRequest(input);
        const next: AuthSession = {
          token: response.token,
          user: response.user,
        };
        writeSession(next);
        clearTeamSelection();
        setSession(next);
      },
      async signup(input) {
        const response = await signupRequest(input);
        const next: AuthSession = {
          token: response.token,
          user: response.user,
        };
        writeSession(next);
        clearTeamSelection();
        setSession(next);
      },
      async logout() {
        try {
          await logoutRequest();
        } finally {
          clearSession();
          clearTeamSelection();
          setSession(null);
        }
      },
    };
  }, [session]);

  return <AuthContext.Provider value={value}>{children}</AuthContext.Provider>;
}

export function useAuth(): AuthContextValue {
  const context = useContext(AuthContext);
  if (!context) {
    throw new Error('useAuth must be used within AuthProvider');
  }
  return context;
}

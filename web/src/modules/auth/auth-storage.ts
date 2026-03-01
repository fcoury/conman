import type { AuthSession, AuthUserSummary } from './auth-types';

const TOKEN_KEY = 'conman.auth.token';
const USER_KEY = 'conman.auth.user';
const TEAM_KEY = 'conman.context.team_id';

function readUser(): AuthUserSummary | null {
  const raw = localStorage.getItem(USER_KEY);
  if (!raw) {
    return null;
  }

  try {
    return JSON.parse(raw) as AuthUserSummary;
  } catch {
    return null;
  }
}

export function readSession(): AuthSession | null {
  const token = localStorage.getItem(TOKEN_KEY);
  if (!token) {
    return null;
  }

  const user = readUser();
  if (!user) {
    return null;
  }

  return { token, user };
}

export function writeSession(session: AuthSession): void {
  localStorage.setItem(TOKEN_KEY, session.token);
  localStorage.setItem(USER_KEY, JSON.stringify(session.user));
}

export function clearSession(): void {
  localStorage.removeItem(TOKEN_KEY);
  localStorage.removeItem(USER_KEY);
}

// Force team re-selection after a fresh sign-in event.
export function clearTeamSelection(): void {
  localStorage.removeItem(TEAM_KEY);
}

export interface AuthUserSummary {
  id: string;
  email: string;
  name: string;
}

export interface LoginInput {
  email: string;
  password: string;
}

export interface LoginResponse {
  token: string;
  user: AuthUserSummary;
}

export interface SignupInput {
  name: string;
  email: string;
  password: string;
}

export interface SignupResponse {
  token: string;
  user: AuthUserSummary;
}

export interface AuthSession {
  token: string;
  user: AuthUserSummary;
}

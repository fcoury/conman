import { apiData } from '~/api/client';

import type {
  LoginInput,
  LoginResponse,
  SignupInput,
  SignupResponse,
} from './auth-types';

export function login(input: LoginInput): Promise<LoginResponse> {
  return apiData<LoginResponse>('/api/auth/login', {
    method: 'POST',
    body: JSON.stringify(input),
  });
}

export async function signup(input: SignupInput): Promise<SignupResponse> {
  const response = await apiData<{
    token: string;
    user: SignupResponse['user'];
  }>('/api/auth/signup', {
    method: 'POST',
    body: JSON.stringify(input),
  });

  return {
    token: response.token,
    user: response.user,
  };
}

export async function logout(): Promise<void> {
  await apiData<{ message: string }>('/api/auth/logout', {
    method: 'POST',
  });
}

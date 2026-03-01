import type { ApiErrorEnvelope, ApiResponseEnvelope } from '~/types/api';

const TOKEN_KEY = 'conman.auth.token';

export class ApiError extends Error {
  code: string;
  requestId: string;
  status: number;

  constructor(code: string, message: string, requestId: string, status: number) {
    super(message);
    this.name = 'ApiError';
    this.code = code;
    this.requestId = requestId;
    this.status = status;
  }
}

// Fetch with JSON handling and auth header
export async function apiRequest<T>(
  path: string,
  init?: RequestInit,
): Promise<ApiResponseEnvelope<T>> {
  const token = localStorage.getItem(TOKEN_KEY);
  const headers: Record<string, string> = {
    'Content-Type': 'application/json',
    ...(init?.headers as Record<string, string>),
  };
  if (token) {
    headers['Authorization'] = `Bearer ${token}`;
  }

  const res = await fetch(path, { ...init, headers });

  if (!res.ok) {
    const body = (await res.json().catch(() => null)) as ApiErrorEnvelope | null;
    throw new ApiError(
      body?.error.code ?? 'UNKNOWN',
      body?.error.message ?? res.statusText,
      body?.error.request_id ?? '',
      res.status,
    );
  }

  return (await res.json()) as ApiResponseEnvelope<T>;
}

// Convenience: unwrap to just the data field
export async function apiData<T>(
  path: string,
  init?: RequestInit,
): Promise<T> {
  const envelope = await apiRequest<T>(path, init);
  return envelope.data;
}

import type { ApiErrorEnvelope, ApiResponseEnvelope } from "@/types/api";

export class ApiError extends Error {
  status: number;
  code?: string;
  requestId?: string;

  constructor(message: string, status: number, code?: string, requestId?: string) {
    super(message);
    this.name = "ApiError";
    this.status = status;
    this.code = code;
    this.requestId = requestId;
  }
}

function buildHeaders(token?: string, contentType = true): Headers {
  const headers = new Headers();
  if (contentType) {
    headers.set("content-type", "application/json");
  }
  if (token) {
    headers.set("authorization", `Bearer ${token}`);
  }
  return headers;
}

export async function apiRequest<T>(
  path: string,
  init: RequestInit = {},
  token?: string,
): Promise<ApiResponseEnvelope<T>> {
  const isFormData = init.body instanceof FormData;
  const response = await fetch(path, {
    ...init,
    headers: {
      ...Object.fromEntries(buildHeaders(token, !isFormData).entries()),
      ...(init.headers ?? {}),
    },
  });

  const text = await response.text();
  const hasJson = text.trim().length > 0;
  const payload = hasJson ? JSON.parse(text) : null;

  if (!response.ok) {
    const errorPayload = payload as ApiErrorEnvelope | null;
    throw new ApiError(
      errorPayload?.error?.message ?? `request failed with status ${response.status}`,
      response.status,
      errorPayload?.error?.code,
      errorPayload?.error?.request_id,
    );
  }

  return payload as ApiResponseEnvelope<T>;
}

export async function apiData<T>(
  path: string,
  init: RequestInit = {},
  token?: string,
): Promise<T> {
  const envelope = await apiRequest<T>(path, init, token);
  return envelope.data;
}

export async function apiPaginated<T>(
  path: string,
  init: RequestInit = {},
  token?: string,
): Promise<ApiResponseEnvelope<T>> {
  return apiRequest<T>(path, init, token);
}

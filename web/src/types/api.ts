export interface PaginationMeta {
  page: number;
  limit: number;
  total: number;
}

export interface ApiResponseEnvelope<T> {
  data: T;
  pagination?: PaginationMeta;
}

export interface ApiErrorBody {
  code: string;
  message: string;
  request_id: string;
}

export interface ApiErrorEnvelope {
  error: ApiErrorBody;
}

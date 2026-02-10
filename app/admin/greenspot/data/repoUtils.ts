import type { PostgrestError } from "@supabase/supabase-js";

import type {
  PaginationParams,
  RepoError,
  RepoListResult,
  RepoResult,
  ResolvedPagination,
} from "./types";

const DEFAULT_PAGE = 1;
const DEFAULT_PAGE_SIZE = 25;

export const resolvePagination = (
  pagination?: PaginationParams,
): ResolvedPagination => {
  const page = pagination?.page && pagination.page > 0 ? pagination.page : 1;
  const pageSize =
    pagination?.pageSize && pagination.pageSize > 0
      ? pagination.pageSize
      : DEFAULT_PAGE_SIZE;
  const from = (page - 1) * pageSize;
  const to = from + pageSize - 1;

  return { page, pageSize, from, to };
};

export const toRepoError = (
  error: PostgrestError | null,
  source: string,
): RepoError | null => {
  if (!error) {
    return null;
  }

  return {
    message: error.message,
    code: error.code,
    details: error.details,
    hint: error.hint,
    source,
  };
};

export const buildRepoResult = <T>(
  data: T | null,
  error: PostgrestError | null,
  source: string,
): RepoResult<T> => ({
  data: error ? null : data,
  error: toRepoError(error, source),
});

export const buildRepoListResult = <T>(
  data: T[] | null,
  count: number | null,
  pagination: ResolvedPagination,
  error: PostgrestError | null,
  source: string,
): RepoListResult<T> => ({
  data: error
    ? null
    : {
        records: data ?? [],
        total: count,
        page: pagination.page,
        pageSize: pagination.pageSize,
      },
  error: toRepoError(error, source),
});

export const defaultPagination = (): ResolvedPagination =>
  resolvePagination({ page: DEFAULT_PAGE, pageSize: DEFAULT_PAGE_SIZE });

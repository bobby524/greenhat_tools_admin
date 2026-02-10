import type { PaginationParams, ResolvedPagination } from "./types";
import { resolvePagination } from "./repoUtils";

type QueryLike<TQuery> = {
  is: (column: string, value: unknown) => TQuery;
  order: (column: string, options?: { ascending?: boolean }) => TQuery;
  range: (from: number, to: number) => TQuery;
};

export const applyArchiveFilter = <
  TQuery extends QueryLike<TQuery>,
  TRecord extends { archived_at: string | null },
>(
  query: TQuery,
  includeArchived?: boolean,
) => {
  if (includeArchived) {
    return query;
  }

  return query.is("archived_at", null);
};

export const applySort = <TQuery extends QueryLike<TQuery>>(
  query: TQuery,
  sort?: { field: string; direction?: "asc" | "desc" },
) => {
  if (!sort?.field) {
    return query;
  }

  return query.order(sort.field, {
    ascending: sort.direction !== "desc",
  });
};

export const applyPagination = <TQuery extends QueryLike<TQuery>>(
  query: TQuery,
  pagination?: PaginationParams,
) => {
  const resolved = resolvePagination(pagination);
  return {
    query: query.range(resolved.from, resolved.to),
    pagination: resolved,
  };
};

import { applyPagination, applySort } from "./queryHelpers";
import { buildRepoListResult } from "./repoUtils";
import { getSupabaseClient } from "./supabaseClient";
import type { AuditEvent } from "./types";

export type AuditEventFilters = {
  entityType?: string;
  entityId?: string;
  actorId?: string;
  eventType?: string;
  fromDate?: string;
  toDate?: string;
};

export type AuditEventSortField = "created_at";

const SOURCE = "crm.auditEventsRepo";

const list = async (
  options: {
    filters?: AuditEventFilters;
    pagination?: { page?: number; pageSize?: number };
    sort?: { field: AuditEventSortField; direction?: "asc" | "desc" };
  } = {},
) => {
  const supabase = getSupabaseClient();
  let query = supabase
    .from("crm_audit_events")
    .select("*", { count: "exact" });

  if (options.filters?.entityType) {
    query = query.eq("entity_type", options.filters.entityType);
  }

  if (options.filters?.entityId) {
    query = query.eq("entity_id", options.filters.entityId);
  }

  if (options.filters?.actorId) {
    query = query.eq("actor_id", options.filters.actorId);
  }

  if (options.filters?.eventType) {
    query = query.eq("event_type", options.filters.eventType);
  }

  if (options.filters?.fromDate) {
    query = query.gte("created_at", options.filters.fromDate);
  }

  if (options.filters?.toDate) {
    query = query.lte("created_at", options.filters.toDate);
  }

  query = options.sort
    ? applySort(query, options.sort)
    : query.order("created_at", { ascending: false });

  const { query: rangedQuery, pagination } = applyPagination(
    query,
    options.pagination,
  );
  const { data, error, count } = await rangedQuery;

  return buildRepoListResult<AuditEvent>(
    data as AuditEvent[] | null,
    count,
    pagination,
    error,
    SOURCE,
  );
};

export const auditEventsRepo = {
  list,
};

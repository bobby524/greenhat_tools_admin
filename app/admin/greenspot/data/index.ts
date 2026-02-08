export { auditEventsRepo } from "./auditEventsRepo";
export type { AuditEventFilters, AuditEventSortField } from "./auditEventsRepo";

export {
  fieldTypes,
  getEntityStorageKey,
  initialContactFields,
  initialContactSections,
  loadEntitySettings,
  loadStoredValue,
  saveEntitySettings,
  saveStoredValue,
} from "./customization";
export type {
  FieldDefinition,
  FieldType,
  SectionDefinition,
} from "./customization";

export {
  defaultDealPipelines,
  loadDealPipelines,
  loadStoredPipelineSelection,
  saveDealPipelines,
  saveStoredPipelineSelection,
} from "./pipelineData";
export type { DealPipeline, DealPipelineStage } from "./pipelineData";

// Note: queryHelpers and repoUtils are kept for internal use but not exported
// as they're not currently used by any components

export { getSupabaseClient } from "./supabaseClient";

export type {
  Json,
  Database,
  Activity,
  AuditEvent,
  Company,
  Contact,
  Deal,
  DealStage,
  RepoError,
  RepoListResponse,
  RepoListResult,
  RepoResult,
  PaginationParams,
  ResolvedPagination,
  Task,
} from "./types";

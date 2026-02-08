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

export { applyArchiveFilter, applySort, applyPagination } from "./queryHelpers";

export {
  toRepoError,
  buildRepoResult,
  buildRepoListResult,
  defaultPagination,
} from "./repoUtils";

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

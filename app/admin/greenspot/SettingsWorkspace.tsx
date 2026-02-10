"use client";

import { useEffect, useState } from "react";
import CrmInlineError from "./components/CrmInlineError";
import { DealPipelineSettings } from "./components/DealPipelineSettings";
import { EntitySettingsEditor } from "./components/EntitySettingsEditor";
import { auditEventsRepo } from "./data/auditEventsRepo";
import type { AuditEvent } from "./data/types";
import {
  initialContactFields,
  initialContactSections,
  type FieldDefinition,
  type SectionDefinition,
} from "./data/customization";

type AuditFilters = {
  entityType: string;
  actorId: string;
  fromDate: string;
  toDate: string;
};

const auditEntityOptions = [
  { value: "", label: "All records" },
  { value: "crm_contacts", label: "Contacts" },
  { value: "crm_companies", label: "Companies" },
  { value: "crm_deals", label: "Deals" },
  { value: "crm_tasks", label: "Tasks" },
  { value: "crm_activities", label: "Activities" },
];

const initialCompanyFields: FieldDefinition[] = [
  {
    id: "company-name",
    label: "Company name",
    fieldKey: "name",
    type: "text",
    required: true,
  },
  {
    id: "company-domain",
    label: "Domain",
    fieldKey: "domain",
    type: "text",
    required: false,
  },
  {
    id: "company-size",
    label: "Employee count",
    fieldKey: "employee_count",
    type: "number",
    required: false,
  },
  {
    id: "company-industry",
    label: "Industry",
    fieldKey: "industry",
    type: "select",
    required: false,
    options: ["Fintech", "Healthcare", "Retail"],
  },
  {
    id: "company-renewal",
    label: "Renewal date",
    fieldKey: "renewal_date",
    type: "date",
    required: false,
  },
];

const initialCompanySections: SectionDefinition[] = [
  {
    id: "company-overview",
    name: "Overview",
    fieldIds: ["company-name", "company-domain", "company-size"],
  },
  {
    id: "company-insights",
    name: "Insights",
    fieldIds: ["company-industry", "company-renewal"],
  },
];

const initialDealFields: FieldDefinition[] = [
  {
    id: "deal-name",
    label: "Deal name",
    fieldKey: "name",
    type: "text",
    required: true,
  },
  {
    id: "deal-value",
    label: "Deal value",
    fieldKey: "value",
    type: "number",
    required: false,
  },
  {
    id: "deal-close-date",
    label: "Close date",
    fieldKey: "close_date",
    type: "date",
    required: false,
  },
  {
    id: "deal-owner",
    label: "Deal owner",
    fieldKey: "owner_id",
    type: "user",
    required: false,
  },
  {
    id: "deal-type",
    label: "Deal type",
    fieldKey: "deal_type",
    type: "select",
    required: false,
    options: ["New business", "Expansion", "Renewal"],
    enforceOptions: true,
  },
  {
    id: "deal-stakeholders",
    label: "Stakeholders",
    fieldKey: "stakeholders",
    type: "multi_select",
    required: false,
    options: ["Finance", "Legal", "Operations"],
    enforceOptions: true,
  },
];

const initialDealSections: SectionDefinition[] = [
  {
    id: "deal-overview",
    name: "Overview",
    fieldIds: ["deal-name", "deal-value", "deal-close-date"],
  },
  {
    id: "deal-ownership",
    name: "Ownership",
    fieldIds: ["deal-owner", "deal-type", "deal-stakeholders"],
  },
];

const initialTaskFields: FieldDefinition[] = [
  {
    id: "task-title",
    label: "Task title",
    fieldKey: "title",
    type: "text",
    required: true,
  },
  {
    id: "task-due-date",
    label: "Due date",
    fieldKey: "due_date",
    type: "date",
    required: false,
  },
  {
    id: "task-assignee",
    label: "Assignee",
    fieldKey: "assignee_id",
    type: "user",
    required: false,
  },
  {
    id: "task-priority",
    label: "Priority",
    fieldKey: "priority",
    type: "select",
    required: false,
    options: ["Low", "Medium", "High"],
    enforceOptions: true,
  },
  {
    id: "task-tags",
    label: "Tags",
    fieldKey: "tags",
    type: "multi_select",
    required: false,
    options: ["Follow-up", "Paperwork", "Internal"],
    enforceOptions: true,
  },
  {
    id: "task-link",
    label: "Reference URL",
    fieldKey: "reference_url",
    type: "url",
    required: false,
  },
];

const initialTaskSections: SectionDefinition[] = [
  {
    id: "task-basics",
    name: "Task basics",
    fieldIds: ["task-title", "task-due-date", "task-assignee"],
  },
  {
    id: "task-context",
    name: "Context",
    fieldIds: ["task-priority", "task-tags", "task-link"],
  },
];

const initialActivityFields: FieldDefinition[] = [
  {
    id: "activity-type",
    label: "Activity type",
    fieldKey: "activity_type",
    type: "select",
    required: true,
    options: ["Note", "Call", "Meeting", "Email"],
    enforceOptions: true,
  },
  {
    id: "activity-outcome",
    label: "Outcome",
    fieldKey: "outcome",
    type: "select",
    required: false,
    options: ["Positive", "Neutral", "Needs follow-up"],
    enforceOptions: true,
  },
  {
    id: "activity-follow-up",
    label: "Follow-up URL",
    fieldKey: "follow_up_url",
    type: "url",
    required: false,
  },
];

const initialActivitySections: SectionDefinition[] = [
  {
    id: "activity-summary",
    name: "Summary",
    fieldIds: ["activity-type", "activity-outcome"],
  },
  {
    id: "activity-links",
    name: "Links",
    fieldIds: ["activity-follow-up"],
  },
];

type EntitySettings = {
  id: string;
  title: string;
  description: string;
  fields: FieldDefinition[];
  sections: SectionDefinition[];
};

const entitySettings: EntitySettings[] = [
  {
    id: "contacts",
    title: "Contacts",
    description:
      "Define the fields and layout your team captures when managing contacts.",
    fields: initialContactFields,
    sections: initialContactSections,
  },
  {
    id: "companies",
    title: "Companies",
    description:
      "Configure company properties, sections, and the order they appear in records.",
    fields: initialCompanyFields,
    sections: initialCompanySections,
  },
  {
    id: "deals",
    title: "Deals",
    description:
      "Control deal fields, ownership metadata, and section layouts for the pipeline.",
    fields: initialDealFields,
    sections: initialDealSections,
  },
  {
    id: "tasks",
    title: "Tasks",
    description:
      "Customize task fields, assignments, and validation rules for follow-up work.",
    fields: initialTaskFields,
    sections: initialTaskSections,
  },
  {
    id: "activities",
    title: "Activities",
    description:
      "Keep activity logging lightweight with a few optional custom fields.",
    fields: initialActivityFields,
    sections: initialActivitySections,
  },
];

function RecentChangesPanel() {
  const [filters, setFilters] = useState<AuditFilters>({
    entityType: "",
    actorId: "",
    fromDate: "",
    toDate: "",
  });
  const [appliedFilters, setAppliedFilters] = useState(filters);
  const [events, setEvents] = useState<AuditEvent[]>([]);
  const [isLoading, setIsLoading] = useState(true);
  const [errorMessage, setErrorMessage] = useState<string | null>(null);

  useEffect(() => {
    let isActive = true;

    const loadEvents = async () => {
      setIsLoading(true);
      setErrorMessage(null);
      const result = await auditEventsRepo.list({
        filters: {
          entityType: appliedFilters.entityType || undefined,
          actorId: appliedFilters.actorId || undefined,
          fromDate: appliedFilters.fromDate
            ? new Date(`${appliedFilters.fromDate}T00:00:00Z`).toISOString()
            : undefined,
          toDate: appliedFilters.toDate
            ? new Date(`${appliedFilters.toDate}T23:59:59Z`).toISOString()
            : undefined,
        },
        pagination: { page: 1, pageSize: 12 },
        sort: { field: "created_at", direction: "desc" },
      });

      if (!isActive) {
        return;
      }

      if (result.error) {
        setErrorMessage(result.error.message);
        setEvents([]);
      } else {
        setEvents(result.data?.records ?? []);
      }

      setIsLoading(false);
    };

    void loadEvents();

    return () => {
      isActive = false;
    };
  }, [appliedFilters]);

  return (
    <section className="space-y-4 rounded-xl border border-gray-200 bg-white p-6">
      <div className="space-y-2">
        <h3 className="text-xl font-semibold text-gray-900">
          Recent changes
        </h3>
        <p className="text-sm text-gray-600">
          Track key CRM updates across records. Activity is captured server-side
          to prevent actor spoofing.
        </p>
      </div>

      <div className="grid gap-3 md:grid-cols-2">
        <label className="text-sm font-semibold text-gray-900">
          Entity
          <select
            value={filters.entityType}
            onChange={(event) =>
              setFilters((current) => ({
                ...current,
                entityType: event.target.value,
              }))
            }
            className="mt-2 w-full rounded-lg border border-gray-200 bg-white px-3 py-2 text-sm text-gray-700 focus:border-[#62ac4a] focus:outline-none focus:ring-1 focus:ring-[#62ac4a]"
          >
            {auditEntityOptions.map((option) => (
              <option key={option.value} value={option.value}>
                {option.label}
              </option>
            ))}
          </select>
        </label>

        <label className="text-sm font-semibold text-gray-900">
          Actor ID
          <input
            value={filters.actorId}
            onChange={(event) =>
              setFilters((current) => ({
                ...current,
                actorId: event.target.value,
              }))
            }
            placeholder="Supabase user UUID"
            className="mt-2 w-full rounded-lg border border-gray-200 bg-white px-3 py-2 text-sm text-gray-700 placeholder:text-gray-400 focus:border-[#62ac4a] focus:outline-none focus:ring-1 focus:ring-[#62ac4a]"
          />
        </label>

        <label className="text-sm font-semibold text-gray-900">
          From
          <input
            type="date"
            value={filters.fromDate}
            onChange={(event) =>
              setFilters((current) => ({
                ...current,
                fromDate: event.target.value,
              }))
            }
            className="mt-2 w-full rounded-lg border border-gray-200 bg-white px-3 py-2 text-sm text-gray-700 focus:border-[#62ac4a] focus:outline-none focus:ring-1 focus:ring-[#62ac4a]"
          />
        </label>

        <label className="text-sm font-semibold text-gray-900">
          To
          <input
            type="date"
            value={filters.toDate}
            onChange={(event) =>
              setFilters((current) => ({
                ...current,
                toDate: event.target.value,
              }))
            }
            className="mt-2 w-full rounded-lg border border-gray-200 bg-white px-3 py-2 text-sm text-gray-700 focus:border-[#62ac4a] focus:outline-none focus:ring-1 focus:ring-[#62ac4a]"
          />
        </label>
      </div>

      <div className="flex flex-wrap items-center gap-2">
        <button
          type="button"
          onClick={() => setAppliedFilters(filters)}
          className="rounded-full border border-[#62ac4a] bg-[#e8f5e9] px-4 py-2 text-sm font-semibold text-[#41734a] transition hover:bg-[#62ac4a] hover:text-white"
        >
          Apply filters
        </button>
        <button
          type="button"
          onClick={() => {
            const reset = {
              entityType: "",
              actorId: "",
              fromDate: "",
              toDate: "",
            };
            setFilters(reset);
            setAppliedFilters(reset);
          }}
          className="rounded-full border border-gray-200 bg-white px-4 py-2 text-sm font-semibold text-gray-600 transition hover:border-[#62ac4a] hover:text-[#62ac4a]"
        >
          Reset
        </button>
      </div>

      {errorMessage ? (
        <CrmInlineError message={errorMessage} />
      ) : (
        <div className="overflow-hidden rounded-xl border border-gray-200">
          <table className="w-full text-left text-sm text-gray-600">
            <thead className="bg-gray-50 text-xs font-bold uppercase tracking-wider text-gray-900">
              <tr>
                <th className="px-4 py-4">Timestamp</th>
                <th className="px-4 py-4">Entity</th>
                <th className="px-4 py-4">Event</th>
                <th className="px-4 py-4">Actor</th>
                <th className="px-4 py-4">Details</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-gray-200">
              {isLoading ? (
                <tr>
                  <td
                    colSpan={5}
                    className="px-4 py-6 text-center text-sm text-gray-600"
                  >
                    Loading audit activity…
                  </td>
                </tr>
              ) : events.length === 0 ? (
                <tr>
                  <td
                    colSpan={5}
                    className="px-4 py-6 text-center text-sm text-gray-600"
                  >
                    No audit events match the current filters.
                  </td>
                </tr>
              ) : (
                events.map((event) => (
                  <tr key={event.id}>
                    <td className="px-4 py-4 font-medium text-gray-900">
                      {new Date(event.created_at).toLocaleString()}
                    </td>
                    <td className="px-4 py-4">
                      <div className="font-semibold text-gray-900">
                        {event.entity_type.replace("crm_", "")}
                      </div>
                      <div className="text-xs text-gray-500">
                        {event.entity_id}
                      </div>
                    </td>
                    <td className="px-4 py-4">{event.event_type}</td>
                    <td className="px-4 py-4">
                      {event.actor_id ?? "System"}
                    </td>
                    <td className="px-4 py-4 text-xs text-gray-500">
                      {event.payload ? JSON.stringify(event.payload) : "—"}
                    </td>
                  </tr>
                ))
              )}
            </tbody>
          </table>
        </div>
      )}
    </section>
  );
}

export default function SettingsWorkspace() {
  return (
    <section className="space-y-10">
      <div className="space-y-2">
        <h2 className="text-2xl font-semibold text-gray-900">
          Admin workspace
        </h2>
        <p className="text-sm text-gray-600">
          Manage CRM field definitions and layout sections for contacts,
          companies, deals, tasks, and activities. Changes are saved to the 
          Supabase database and immediately reflected on the tools site.
        </p>
      </div>

      <div className="space-y-10">
        <RecentChangesPanel />
        <DealPipelineSettings />
        {entitySettings.map((entity) => (
          <EntitySettingsEditor key={entity.id} {...entity} />
        ))}
      </div>
    </section>
  );
}

"use client";

import { useEffect, useMemo, useState } from "react";
import CrmInlineError from "./components/CrmInlineError";
import {
  fieldTypes,
  getEntityStorageKey,
  initialContactFields,
  initialContactSections,
  loadEntitySettings,
  loadStoredValue,
  saveEntitySettings,
  saveStoredValue,
  type FieldDefinition,
  type FieldType,
  type SectionDefinition,
} from "./data/customization";
import {
  defaultDealPipelines,
  loadDealPipelines,
  loadStoredPipelineSelection,
  saveDealPipelines,
  saveStoredPipelineSelection,
  type DealPipeline,
  type DealPipelineStage,
} from "./data/pipelineData";
import { auditEventsRepo } from "./data/auditEventsRepo";
import type { AuditEvent } from "./data/types";

type DealStage = DealPipelineStage & {
  dealsCount: number;
};

type FieldDraft = {
  label: string;
  type: FieldType;
  required: boolean;
  options: string;
  enforceOptions: boolean;
};

const createId = (prefix: string) =>
  `${prefix}-${globalThis.crypto?.randomUUID?.() ?? Math.random().toString(36).slice(2)}`;

const typeLabel: Record<FieldType, string> = {
  text: "Text",
  number: "Number",
  date: "Date",
  boolean: "Boolean",
  select: "Select",
  multi_select: "Multi-select",
  user: "User",
  url: "URL",
};

const fieldTypesWithOptions = new Set<FieldType>(["select", "multi_select"]);

const validateFieldDraft = (draft: FieldDraft) => {
  if (!draft.label.trim()) {
    return "Field label is required.";
  }
  if (fieldTypesWithOptions.has(draft.type)) {
    const options = draft.options
      .split(",")
      .map((option) => option.trim())
      .filter(Boolean);
    if (options.length === 0) {
      return "Provide at least one option for select fields.";
    }
  }

  return null;
};

const normalizeFieldKey = (label: string) => {
  const normalized = label
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "_")
    .replace(/^_+|_+$/g, "");

  return normalized.length > 0 ? normalized : "field";
};

const createUniqueFieldKey = (label: string, fields: FieldDefinition[]) => {
  const base = normalizeFieldKey(label);
  const usedKeys = new Set(
    fields.map((field) => field.fieldKey.trim().toLowerCase()),
  );
  if (!usedKeys.has(base)) {
    return base;
  }
  let suffix = 2;
  while (usedKeys.has(`${base}_${suffix}`)) {
    suffix += 1;
  }
  return `${base}_${suffix}`;
};

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

type StageDraft = {
  label: string;
  key: string;
  isClosedWon: boolean;
  isClosedLost: boolean;
};

type PipelineDraft = {
  name: string;
  description: string;
};

const formatStageKey = (label: string) =>
  label
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "_")
    .replace(/^_+|_+$/g, "");

const formatPipelineId = (label: string) =>
  label
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "");

const normalizeText = (value: string) => value.trim().toLowerCase();

function SectionEditor({
  sections,
  setSections,
  fields,
}: {
  sections: SectionDefinition[];
  setSections: React.Dispatch<React.SetStateAction<SectionDefinition[]>>;
  fields: FieldDefinition[];
}) {
  const fieldLookup = useMemo(
    () =>
      fields.reduce<Record<string, FieldDefinition>>((acc, field) => {
        acc[field.id] = field;
        return acc;
      }, {}),
    [fields]
  );

  const moveSection = (sectionId: string, direction: "up" | "down") => {
    setSections((current) => {
      const index = current.findIndex((section) => section.id === sectionId);
      if (index < 0) return current;
      const nextIndex = direction === "up" ? index - 1 : index + 1;
      if (nextIndex < 0 || nextIndex >= current.length) return current;
      const updated = [...current];
      const [removed] = updated.splice(index, 1);
      updated.splice(nextIndex, 0, removed);
      return updated;
    });
  };

  const moveField = (
    sectionId: string,
    fieldId: string,
    direction: "up" | "down"
  ) => {
    setSections((current) =>
      current.map((section) => {
        if (section.id !== sectionId) return section;
        const index = section.fieldIds.indexOf(fieldId);
        if (index < 0) return section;
        const nextIndex = direction === "up" ? index - 1 : index + 1;
        if (nextIndex < 0 || nextIndex >= section.fieldIds.length)
          return section;
        const nextFieldIds = [...section.fieldIds];
        const [removed] = nextFieldIds.splice(index, 1);
        nextFieldIds.splice(nextIndex, 0, removed);
        return { ...section, fieldIds: nextFieldIds };
      })
    );
  };

  return (
    <div className="space-y-4">
      {sections.map((section, sectionIndex) => (
        <div
          key={section.id}
          className="rounded-xl border border-gray-200 bg-gray-50 p-4"
        >
          <div className="flex flex-wrap items-center justify-between gap-3">
            <div>
              <p className="text-xs font-semibold uppercase tracking-wider text-[#62ac4a]">
                Section {sectionIndex + 1}
              </p>
              <input
                value={section.name}
                onChange={(event) => {
                  const value = event.target.value;
                  setSections((current) =>
                    current.map((item) =>
                      item.id === section.id ? { ...item, name: value } : item
                    )
                  );
                }}
                className="mt-2 w-full rounded-lg border border-gray-200 bg-white px-3 py-2 text-sm font-semibold text-gray-900 focus:border-[#62ac4a] focus:outline-none focus:ring-1 focus:ring-[#62ac4a]"
                aria-label="Section name"
              />
            </div>
            <div className="flex items-center gap-2 text-xs font-semibold text-gray-600">
              <button
                type="button"
                onClick={() => moveSection(section.id, "up")}
                className="rounded-full border border-gray-200 px-3 py-1 transition hover:border-[#62ac4a] hover:text-[#62ac4a]"
              >
                Move up
              </button>
              <button
                type="button"
                onClick={() => moveSection(section.id, "down")}
                className="rounded-full border border-gray-200 px-3 py-1 transition hover:border-[#62ac4a] hover:text-[#62ac4a]"
              >
                Move down
              </button>
            </div>
          </div>
          <div className="mt-4 space-y-2">
            {section.fieldIds.length === 0 ? (
              <p className="text-sm text-gray-500">
                No fields assigned yet. Use the selector in the field list to
                assign one.
              </p>
            ) : (
              section.fieldIds.map((fieldId) => {
                const field = fieldLookup[fieldId];
                if (!field) return null;
                return (
                  <div
                    key={fieldId}
                    className="flex flex-wrap items-center justify-between gap-2 rounded-lg border border-gray-200 bg-white px-3 py-2"
                  >
                    <div>
                      <p className="text-sm font-semibold text-gray-900">
                        {field.label}
                      </p>
                      <p className="text-xs text-gray-500">
                        {typeLabel[field.type]}
                      </p>
                    </div>
                    <div className="flex items-center gap-2 text-xs font-semibold text-gray-600">
                      <button
                        type="button"
                        onClick={() => moveField(section.id, fieldId, "up")}
                        className="rounded-full border border-gray-200 px-3 py-1 transition hover:border-[#62ac4a] hover:text-[#62ac4a]"
                      >
                        Up
                      </button>
                      <button
                        type="button"
                        onClick={() => moveField(section.id, fieldId, "down")}
                        className="rounded-full border border-gray-200 px-3 py-1 transition hover:border-[#62ac4a] hover:text-[#62ac4a]"
                      >
                        Down
                      </button>
                    </div>
                  </div>
                );
              })
            )}
          </div>
        </div>
      ))}
    </div>
  );
}

function DealPipelineSettings() {
  const [pipelines, setPipelines] =
    useState<DealPipeline[]>(defaultDealPipelines);
  const [activePipelineId, setActivePipelineId] = useState(
    defaultDealPipelines[0]?.id ?? "",
  );
  const [hasLoadedPipelines, setHasLoadedPipelines] = useState(false);
  const [draft, setDraft] = useState<StageDraft>({
    label: "",
    key: "",
    isClosedWon: false,
    isClosedLost: false,
  });
  const [pipelineDraft, setPipelineDraft] = useState<PipelineDraft>({
    name: "",
    description: "",
  });
  const [draftError, setDraftError] = useState<string | null>(null);
  const [pipelineError, setPipelineError] = useState<string | null>(null);
  const [pipelinePersistError, setPipelinePersistError] = useState<
    string | null
  >(null);
  const [pipelineDeleteError, setPipelineDeleteError] = useState<string | null>(
    null,
  );
  const [editingKey, setEditingKey] = useState<string | null>(null);
  const [editLabel, setEditLabel] = useState("");
  const [editError, setEditError] = useState<string | null>(null);

  const activePipeline = useMemo(
    () =>
      pipelines.find((pipeline) => pipeline.id === activePipelineId) ??
      pipelines[0] ??
      null,
    [activePipelineId, pipelines],
  );

  const stageDealCounts = useMemo<Record<string, number>>(() => ({}), []);

  const stages = useMemo<DealStage[]>(
    () =>
      (activePipeline?.stages ?? []).map((stage) => ({
        ...stage,
        dealsCount: stageDealCounts[stage.label] ?? 0,
      })),
    [activePipeline, stageDealCounts],
  );

  const sortedStages = [...stages].sort((a, b) => a.order - b.order);

  const updateStageOrder = (nextStages: DealPipelineStage[]) =>
    nextStages.map((stage, index) => ({ ...stage, order: index + 1 }));

  const updatePipelineStages = (
    updater: (current: DealPipelineStage[]) => DealPipelineStage[],
  ) => {
    setPipelines((current) =>
      current.map((pipeline) =>
        pipeline.id === activePipelineId
          ? { ...pipeline, stages: updater(pipeline.stages) }
          : pipeline,
      ),
    );
  };

  useEffect(() => {
    if (hasLoadedPipelines) {
      return;
    }
    const storedSelection = loadStoredPipelineSelection();
    const loadPipelines = async () => {
      const loadedPipelines =
        (await loadDealPipelines()) ?? defaultDealPipelines;
      const nextPipelineId =
        storedSelection &&
        loadedPipelines.some((pipeline) => pipeline.id === storedSelection)
          ? storedSelection
          : loadedPipelines[0]?.id ?? "";
      setPipelines(loadedPipelines);
      setActivePipelineId(nextPipelineId);
      setHasLoadedPipelines(true);
    };
    void loadPipelines();
  }, [hasLoadedPipelines]);

  useEffect(() => {
    if (!hasLoadedPipelines) {
      return;
    }
    const persist = async () => {
      const result = await saveDealPipelines(pipelines);
      setPipelinePersistError(result.error?.message ?? null);
    };
    void persist();
  }, [hasLoadedPipelines, pipelines]);

  useEffect(() => {
    if (!hasLoadedPipelines || !activePipelineId) {
      return;
    }
    saveStoredPipelineSelection(activePipelineId);
    setDraft({
      label: "",
      key: "",
      isClosedWon: false,
      isClosedLost: false,
    });
    setDraftError(null);
    setEditingKey(null);
    setEditLabel("");
    setEditError(null);
    setPipelineDeleteError(null);
    setPipelinePersistError(null);
  }, [activePipelineId, hasLoadedPipelines]);

  const handleMoveStage = (stageKey: string, direction: "up" | "down") => {
    updatePipelineStages((current) => {
      const ordered = [...current].sort((a, b) => a.order - b.order);
      const index = ordered.findIndex((stage) => stage.key === stageKey);
      if (index < 0) return current;
      const nextIndex = direction === "up" ? index - 1 : index + 1;
      if (nextIndex < 0 || nextIndex >= ordered.length) return current;
      const nextStages = [...ordered];
      const [moved] = nextStages.splice(index, 1);
      nextStages.splice(nextIndex, 0, moved);
      return updateStageOrder(nextStages);
    });
  };

  const handleToggleClosed = (
    stageKey: string,
    flag: "won" | "lost",
    checked: boolean
  ) => {
    updatePipelineStages((current) =>
      current.map((stage) => {
        if (stage.key !== stageKey) return stage;
        if (flag === "won") {
          return {
            ...stage,
            isClosedWon: checked,
            isClosedLost: checked ? false : stage.isClosedLost,
          };
        }
        return {
          ...stage,
          isClosedLost: checked,
          isClosedWon: checked ? false : stage.isClosedWon,
        };
      })
    );
  };

  const handleToggleActive = (stageKey: string) => {
    updatePipelineStages((current) =>
      current.map((stage) =>
        stage.key === stageKey ? { ...stage, isActive: !stage.isActive } : stage
      )
    );
  };

  const handleDeleteStage = (stageKey: string) => {
    updatePipelineStages((current) => {
      const remaining = current.filter((stage) => stage.key !== stageKey);
      return updateStageOrder(remaining);
    });
  };

  const handleStartEdit = (stage: DealStage) => {
    setEditingKey(stage.key);
    setEditLabel(stage.label);
    setEditError(null);
  };

  const handleSaveEdit = () => {
    if (!editingKey) return;
    const trimmed = editLabel.trim();
    if (!trimmed) {
      setEditError("Stage label is required.");
      return;
    }
    const isDuplicate = stages.some(
      (stage) =>
        stage.key !== editingKey &&
        normalizeText(stage.label) === normalizeText(trimmed)
    );
    if (isDuplicate) {
      setEditError("Stage labels must be unique.");
      return;
    }
    updatePipelineStages((current) =>
      current.map((stage) =>
        stage.key === editingKey ? { ...stage, label: trimmed } : stage
      )
    );
    setEditingKey(null);
    setEditLabel("");
  };

  const handleCancelEdit = () => {
    setEditingKey(null);
    setEditLabel("");
    setEditError(null);
  };

  const handleAddStage = () => {
    const label = draft.label.trim();
    if (!label) {
      setDraftError("Stage label is required.");
      return;
    }
    const keySource = draft.key.trim() || formatStageKey(label);
    if (!keySource) {
      setDraftError("Stage key is required.");
      return;
    }
    const key = keySource.toLowerCase();
    const labelConflict = stages.some(
      (stage) => normalizeText(stage.label) === normalizeText(label)
    );
    const keyConflict = stages.some(
      (stage) => normalizeText(stage.key) === normalizeText(key)
    );
    if (labelConflict) {
      setDraftError("Stage labels must be unique.");
      return;
    }
    if (keyConflict) {
      setDraftError("Stage keys must be unique.");
      return;
    }

    updatePipelineStages((current) =>
      updateStageOrder([
        ...current,
        {
          id: createId("stage"),
          key,
          label,
          order: current.length + 1,
          isClosedWon: draft.isClosedWon,
          isClosedLost: draft.isClosedLost,
          isActive: true,
        },
      ])
    );
    setDraft({
      label: "",
      key: "",
      isClosedWon: false,
      isClosedLost: false,
    });
    setDraftError(null);
  };

  const handleAddPipeline = () => {
    const name = pipelineDraft.name.trim();
    if (!name) {
      setPipelineError("Pipeline name is required.");
      return;
    }
    const id = formatPipelineId(name);
    if (!id) {
      setPipelineError("Pipeline ID is required.");
      return;
    }
    const nameConflict = pipelines.some(
      (pipeline) => normalizeText(pipeline.name) === normalizeText(name),
    );
    if (nameConflict) {
      setPipelineError("Pipeline names must be unique.");
      return;
    }
    const idConflict = pipelines.some(
      (pipeline) => normalizeText(pipeline.id) === normalizeText(id),
    );
    if (idConflict) {
      setPipelineError("Pipeline IDs must be unique.");
      return;
    }

    const seedStages =
      defaultDealPipelines[0]?.stages.map((stage, index) => ({
        ...stage,
        order: index + 1,
      })) ?? [];

    const nextPipeline: DealPipeline = {
      id,
      name,
      description: pipelineDraft.description.trim(),
      stages: seedStages,
    };

    setPipelines((current) => [...current, nextPipeline]);
    setActivePipelineId(id);
    setPipelineDraft({ name: "", description: "" });
    setPipelineError(null);
  };

  const handleDeletePipeline = () => {
    if (!activePipeline) {
      return;
    }
    if (pipelines.length <= 1) {
      setPipelineDeleteError("Keep at least one pipeline active.");
      return;
    }
    const confirmed = window.confirm(
      `Delete the ${activePipeline.name} pipeline? This will remove it from the admin list.`,
    );
    if (!confirmed) {
      return;
    }
    const remaining = pipelines.filter(
      (pipeline) => pipeline.id !== activePipeline.id,
    );
    setPipelines(remaining);
    setActivePipelineId(remaining[0]?.id ?? "");
    setPipelineDeleteError(null);
    setPipelinePersistError(null);
  };

  return (
    <section className="space-y-6">
      <div className="space-y-2">
        <h3 className="text-xl font-semibold text-gray-900">
          Pipeline stages & deal settings
        </h3>
        <p className="text-sm text-gray-600">
          Configure the order, status, and close-out flags for deal stages.
          Stable keys keep reporting and automation consistent.
        </p>
      </div>

      <div className="grid gap-6 lg:grid-cols-[minmax(0,2fr)_minmax(0,1fr)]">
        <div className="space-y-4 rounded-xl border border-gray-200 bg-white p-6">
          <div>
            <h4 className="text-sm font-semibold uppercase tracking-wider text-[#62ac4a]">
              Pipeline selection
            </h4>
            <p className="text-sm text-gray-600">
              Switch between pipelines to edit their stage configurations.
            </p>
          </div>

          <div className="space-y-3">
            <label className="text-xs font-semibold uppercase tracking-wider text-[#62ac4a]">
              Active pipeline
            </label>
            <select
              value={activePipeline?.id ?? ""}
              onChange={(event) => setActivePipelineId(event.target.value)}
              className="w-full rounded-lg border border-gray-200 bg-white px-3 py-2 text-sm text-gray-700 focus:border-[#62ac4a] focus:outline-none focus:ring-1 focus:ring-[#62ac4a]"
              aria-label="Select pipeline"
            >
              {pipelines.map((pipeline) => (
                <option key={pipeline.id} value={pipeline.id}>
                  {pipeline.name}
                </option>
              ))}
            </select>
            {activePipeline ? (
              <div className="rounded-lg border border-gray-200 bg-gray-50 p-3">
                <p className="text-sm font-semibold text-gray-900">
                  {activePipeline.name}
                </p>
                <p className="text-xs text-gray-600">
                  {activePipeline.description || "No description yet."}
                </p>
                <p className="mt-2 text-xs text-gray-600">
                  {activePipeline.stages.length} stage
                  {activePipeline.stages.length === 1 ? "" : "s"} configured
                  for this pipeline.
                </p>
                <div className="mt-3 space-y-2">
                  <button
                    type="button"
                    onClick={handleDeletePipeline}
                    disabled={pipelines.length <= 1}
                    className="w-full rounded-full border border-gray-200 px-3 py-1 text-xs font-semibold text-gray-600 transition hover:border-[#62ac4a] hover:text-[#62ac4a] disabled:cursor-not-allowed disabled:opacity-60"
                  >
                    Delete pipeline
                  </button>
                  {pipelineDeleteError ? (
                    <p className="text-xs text-red-600">{pipelineDeleteError}</p>
                  ) : null}
                </div>
              </div>
            ) : null}
            {pipelinePersistError ? (
              <CrmInlineError message={pipelinePersistError} />
            ) : null}
          </div>
        </div>

        <div className="space-y-4 rounded-xl border border-gray-200 bg-white p-6">
          <div>
            <h4 className="text-sm font-semibold uppercase tracking-wider text-[#62ac4a]">
              Add pipeline (admin only)
            </h4>
            <p className="text-sm text-gray-600">
              Create additional pipelines for different sales motions or
              customer segments.
            </p>
          </div>

          <div className="space-y-3">
            <input
              value={pipelineDraft.name}
              onChange={(event) =>
                setPipelineDraft((current) => {
                  setPipelineError(null);
                  setPipelinePersistError(null);
                  return {
                    ...current,
                    name: event.target.value,
                  };
                })
              }
              placeholder="Pipeline name"
              className="w-full rounded-lg border border-gray-200 bg-white px-3 py-2 text-sm text-gray-700 placeholder:text-gray-400 focus:border-[#62ac4a] focus:outline-none focus:ring-1 focus:ring-[#62ac4a]"
            />
            <textarea
              value={pipelineDraft.description}
              onChange={(event) =>
                setPipelineDraft((current) => {
                  setPipelineError(null);
                  setPipelinePersistError(null);
                  return {
                    ...current,
                    description: event.target.value,
                  };
                })
              }
              placeholder="Describe who uses this pipeline"
              className="min-h-[96px] w-full rounded-lg border border-gray-200 bg-white px-3 py-2 text-sm text-gray-700 placeholder:text-gray-400 focus:border-[#62ac4a] focus:outline-none focus:ring-1 focus:ring-[#62ac4a]"
            />
            {pipelineError ? (
              <p className="text-xs text-red-600">{pipelineError}</p>
            ) : null}
            <button
              type="button"
              onClick={handleAddPipeline}
              className="w-full rounded-full border border-[#62ac4a] bg-[#e8f5e9] px-4 py-2 text-sm font-semibold text-[#41734a] transition hover:bg-[#62ac4a] hover:text-white"
            >
              Add pipeline
            </button>
          </div>
        </div>
      </div>

      <div className="grid gap-6 lg:grid-cols-[minmax(0,2fr)_minmax(0,1fr)]">
        <div className="space-y-4 rounded-xl border border-gray-200 bg-white p-6">
          <div>
            <h4 className="text-sm font-semibold uppercase tracking-wider text-[#62ac4a]">
              Stage list
            </h4>
            <p className="text-sm text-gray-600">
              Reorder stages and mark closed-won or closed-lost outcomes for{" "}
              <span className="font-semibold text-gray-900">
                {activePipeline?.name ?? "this pipeline"}
              </span>
              .
            </p>
          </div>

          <div className="space-y-3">
            {sortedStages.map((stage) => {
              const isEditing = editingKey === stage.key;
              const canDelete = stage.dealsCount === 0;
              return (
                <div
                  key={stage.key}
                  className="rounded-xl border border-gray-200 bg-gray-50 p-4"
                >
                  <div className="flex flex-wrap items-start justify-between gap-3">
                    <div className="space-y-1">
                      <p className="text-xs font-semibold uppercase tracking-wider text-[#62ac4a]">
                        Stage {stage.order}
                      </p>
                      {isEditing ? (
                        <div>
                          <input
                            value={editLabel}
                            onChange={(event) => {
                              setEditLabel(event.target.value);
                              setEditError(null);
                            }}
                            className="w-full rounded-lg border border-gray-200 bg-white px-3 py-2 text-sm font-semibold text-gray-900 focus:border-[#62ac4a] focus:outline-none focus:ring-1 focus:ring-[#62ac4a]"
                            aria-label="Stage label"
                          />
                          {editError ? (
                            <p className="mt-2 text-xs text-red-600">
                              {editError}
                            </p>
                          ) : null}
                        </div>
                      ) : (
                        <p className="text-lg font-semibold text-gray-900">
                          {stage.label}
                        </p>
                      )}
                      <p className="text-xs text-gray-600">
                        Key:{" "}
                        <span className="font-semibold text-gray-900">
                          {stage.key}
                        </span>
                        {stage.isActive ? "" : " • Disabled"}
                      </p>
                      <p className="text-xs text-gray-600">
                        {stage.dealsCount} active deal
                        {stage.dealsCount === 1 ? "" : "s"}
                      </p>
                    </div>

                    <div className="flex flex-wrap items-center gap-2 text-xs font-semibold text-gray-600">
                      <button
                        type="button"
                        onClick={() => handleMoveStage(stage.key, "up")}
                        className="rounded-full border border-gray-200 px-3 py-1 transition hover:border-[#62ac4a] hover:text-[#62ac4a]"
                      >
                        Move up
                      </button>
                      <button
                        type="button"
                        onClick={() => handleMoveStage(stage.key, "down")}
                        className="rounded-full border border-gray-200 px-3 py-1 transition hover:border-[#62ac4a] hover:text-[#62ac4a]"
                      >
                        Move down
                      </button>
                      {isEditing ? (
                        <>
                          <button
                            type="button"
                            onClick={handleSaveEdit}
                            className="rounded-full border border-[#62ac4a] bg-[#e8f5e9] px-3 py-1 text-[#41734a] transition hover:bg-[#62ac4a] hover:text-white"
                          >
                            Save
                          </button>
                          <button
                            type="button"
                            onClick={handleCancelEdit}
                            className="rounded-full border border-gray-200 px-3 py-1 transition hover:border-[#62ac4a] hover:text-[#62ac4a]"
                          >
                            Cancel
                          </button>
                        </>
                      ) : (
                        <button
                          type="button"
                          onClick={() => handleStartEdit(stage)}
                          className="rounded-full border border-gray-200 px-3 py-1 transition hover:border-[#62ac4a] hover:text-[#62ac4a]"
                        >
                          Rename
                        </button>
                      )}
                      <button
                        type="button"
                        onClick={() => handleToggleActive(stage.key)}
                        className="rounded-full border border-gray-200 px-3 py-1 transition hover:border-[#62ac4a] hover:text-[#62ac4a]"
                      >
                        {stage.isActive ? "Disable" : "Enable"}
                      </button>
                      <button
                        type="button"
                        onClick={() => handleDeleteStage(stage.key)}
                        className="rounded-full border border-gray-200 px-3 py-1 transition hover:border-red-400 hover:text-red-600 disabled:cursor-not-allowed disabled:opacity-60"
                        disabled={!canDelete}
                      >
                        Delete
                      </button>
                    </div>
                  </div>

                  <div className="mt-3 flex flex-wrap items-center gap-4 text-sm text-gray-600">
                    <label className="flex items-center gap-2">
                      <input
                        type="checkbox"
                        checked={stage.isClosedWon}
                        onChange={(event) =>
                          handleToggleClosed(
                            stage.key,
                            "won",
                            event.target.checked
                          )
                        }
                        className="h-4 w-4 rounded border-gray-300 text-[#62ac4a] focus:ring-[#62ac4a]"
                      />
                      Closed won
                    </label>
                    <label className="flex items-center gap-2">
                      <input
                        type="checkbox"
                        checked={stage.isClosedLost}
                        onChange={(event) =>
                          handleToggleClosed(
                            stage.key,
                            "lost",
                            event.target.checked
                          )
                        }
                        className="h-4 w-4 rounded border-gray-300 text-[#62ac4a] focus:ring-[#62ac4a]"
                      />
                      Closed lost
                    </label>
                    {!canDelete ? (
                      <span className="text-xs text-gray-500">
                        In use — disable or migrate deals before deleting.
                      </span>
                    ) : null}
                  </div>
                </div>
              );
            })}
          </div>
        </div>

        <div className="space-y-4 rounded-xl border border-gray-200 bg-white p-6">
          <div>
            <h4 className="text-sm font-semibold uppercase tracking-wider text-[#62ac4a]">
              Add a stage
            </h4>
            <p className="text-sm text-gray-600">
              Stage keys are permanent identifiers used in reports and
              automation.
            </p>
          </div>

          <div className="space-y-3">
            <input
              value={draft.label}
              onChange={(event) =>
                setDraft((current) => ({
                  ...current,
                  label: event.target.value,
                }))
              }
              placeholder="Stage label"
              className="w-full rounded-lg border border-gray-200 bg-white px-3 py-2 text-sm text-gray-700 placeholder:text-gray-400 focus:border-[#62ac4a] focus:outline-none focus:ring-1 focus:ring-[#62ac4a]"
            />
            <input
              value={draft.key}
              onChange={(event) =>
                setDraft((current) => ({
                  ...current,
                  key: event.target.value,
                }))
              }
              placeholder="stage_key (optional)"
              className="w-full rounded-lg border border-gray-200 bg-white px-3 py-2 text-sm text-gray-700 placeholder:text-gray-400 focus:border-[#62ac4a] focus:outline-none focus:ring-1 focus:ring-[#62ac4a]"
            />
            <div className="space-y-2 text-sm text-gray-600">
              <label className="flex items-center gap-2">
                <input
                  type="checkbox"
                  checked={draft.isClosedWon}
                  onChange={(event) =>
                    setDraft((current) => ({
                      ...current,
                      isClosedWon: event.target.checked,
                      isClosedLost: event.target.checked
                        ? false
                        : current.isClosedLost,
                    }))
                  }
                  className="h-4 w-4 rounded border-gray-300 text-[#62ac4a] focus:ring-[#62ac4a]"
                />
                Closed won
              </label>
              <label className="flex items-center gap-2">
                <input
                  type="checkbox"
                  checked={draft.isClosedLost}
                  onChange={(event) =>
                    setDraft((current) => ({
                      ...current,
                      isClosedLost: event.target.checked,
                      isClosedWon: event.target.checked
                        ? false
                        : current.isClosedWon,
                    }))
                  }
                  className="h-4 w-4 rounded border-gray-300 text-[#62ac4a] focus:ring-[#62ac4a]"
                />
                Closed lost
              </label>
            </div>
            {draftError ? (
              <p className="text-xs text-red-600">{draftError}</p>
            ) : null}
            <button
              type="button"
              onClick={handleAddStage}
              className="w-full rounded-full border border-[#62ac4a] bg-[#e8f5e9] px-4 py-2 text-sm font-semibold text-[#41734a] transition hover:bg-[#62ac4a] hover:text-white"
            >
              Add stage
            </button>
          </div>
        </div>
      </div>
    </section>
  );
}

function EntitySettingsEditor({
  id,
  title,
  description,
  fields: initialFields,
  sections: initialSections,
}: EntitySettings) {
  const [fields, setFields] = useState<FieldDefinition[]>(initialFields);
  const [sections, setSections] = useState<SectionDefinition[]>(initialSections);
  const [hasLoadedEntitySettings, setHasLoadedEntitySettings] = useState(false);
  const [draftField, setDraftField] = useState<FieldDraft>({
    label: "",
    type: "text",
    required: false,
    options: "",
    enforceOptions: true,
  });
  const [newSectionName, setNewSectionName] = useState("");
  const [editingFieldId, setEditingFieldId] = useState<string | null>(null);
  const [editDraft, setEditDraft] = useState<FieldDraft | null>(null);
  const [draftError, setDraftError] = useState<string | null>(null);
  const [editError, setEditError] = useState<string | null>(null);

  const activeFields = fields.filter((field) => !field.archived);
  const archivedFields = fields.filter((field) => field.archived);

  useEffect(() => {
    if (hasLoadedEntitySettings) {
      return;
    }
    const loadSettings = async () => {
      const storedSettings = await loadEntitySettings(id);
      if (storedSettings) {
        setFields(storedSettings.fields);
        setSections(storedSettings.sections);
      }
      setHasLoadedEntitySettings(true);
    };

    void loadSettings();
  }, [hasLoadedEntitySettings, id]);

  useEffect(() => {
    if (!hasLoadedEntitySettings) {
      return;
    }
    const nextSettings = {
      fields,
      sections,
    };

    saveStoredValue(getEntityStorageKey(id), nextSettings);
    void saveEntitySettings(id, nextSettings);
  }, [fields, hasLoadedEntitySettings, id, sections]);

  const handleAddField = () => {
    const errorMessage = validateFieldDraft(draftField);
    if (errorMessage) {
      setDraftError(errorMessage);
      return;
    }

    const fieldKey = createUniqueFieldKey(draftField.label, fields);
    const nextField: FieldDefinition = {
      id: createId("field"),
      label: draftField.label.trim(),
      fieldKey,
      type: draftField.type,
      required: draftField.required,
      options:
        fieldTypesWithOptions.has(draftField.type)
          ? draftField.options
              .split(",")
              .map((option) => option.trim())
              .filter(Boolean)
          : undefined,
      enforceOptions: fieldTypesWithOptions.has(draftField.type)
        ? draftField.enforceOptions
        : undefined,
    };

    setFields((current) => [...current, nextField]);
    setSections((current) => {
      if (current.length === 0) {
        return [
          {
            id: createId("section"),
            name: "Overview",
            fieldIds: [nextField.id],
          },
        ];
      }
      return current.map((section, index) =>
        index === 0
          ? { ...section, fieldIds: [...section.fieldIds, nextField.id] }
          : section
      );
    });
    setDraftField({
      label: "",
      type: "text",
      required: false,
      options: "",
      enforceOptions: true,
    });
    setDraftError(null);
  };

  const handleArchiveField = (fieldId: string) => {
    setFields((current) =>
      current.map((field) =>
        field.id === fieldId ? { ...field, archived: true } : field
      )
    );
    setSections((current) =>
      current.map((section) => ({
        ...section,
        fieldIds: section.fieldIds.filter((id) => id !== fieldId),
      }))
    );
  };

  const handleAssignField = (fieldId: string, sectionId: string) => {
    setSections((current) =>
      current.map((section) => {
        if (section.id === sectionId) {
          if (section.fieldIds.includes(fieldId)) return section;
          return { ...section, fieldIds: [...section.fieldIds, fieldId] };
        }
        return {
          ...section,
          fieldIds: section.fieldIds.filter((id) => id !== fieldId),
        };
      })
    );
  };

  const handleAddSection = () => {
    if (!newSectionName.trim()) return;
    setSections((current) => [
      ...current,
      {
        id: createId("section"),
        name: newSectionName.trim(),
        fieldIds: [],
      },
    ]);
    setNewSectionName("");
  };

  const handleEditField = (field: FieldDefinition) => {
    setEditingFieldId(field.id);
    setEditDraft({
      label: field.label,
      type: field.type,
      required: field.required,
      options: field.options?.join(", ") ?? "",
      enforceOptions: field.enforceOptions ?? true,
    });
    setEditError(null);
  };

  const handleSaveEdit = () => {
    if (!editingFieldId || !editDraft) return;
    const errorMessage = validateFieldDraft(editDraft);
    if (errorMessage) {
      setEditError(errorMessage);
      return;
    }

    setFields((current) =>
      current.map((field) =>
        field.id === editingFieldId
          ? {
              ...field,
              label: editDraft.label.trim(),
              type: editDraft.type,
              required: editDraft.required,
              options:
                fieldTypesWithOptions.has(editDraft.type)
                  ? editDraft.options
                      .split(",")
                      .map((option) => option.trim())
                      .filter(Boolean)
                  : undefined,
              enforceOptions: fieldTypesWithOptions.has(editDraft.type)
                ? editDraft.enforceOptions
                : undefined,
            }
          : field
      )
    );
    setEditingFieldId(null);
    setEditDraft(null);
    setEditError(null);
  };

  const handleCancelEdit = () => {
    setEditingFieldId(null);
    setEditDraft(null);
    setEditError(null);
  };

  return (
    <section className="space-y-6">
      <div className="space-y-2">
        <h3 className="text-xl font-semibold text-gray-900">{title}</h3>
        <p className="text-sm text-gray-600">{description}</p>
      </div>

      <div className="grid gap-6 lg:grid-cols-2">
        <div className="space-y-4 rounded-xl border border-gray-200 bg-white p-6">
          <div>
            <h4 className="text-sm font-semibold uppercase tracking-wider text-[#62ac4a]">
              Field definitions
            </h4>
            <p className="text-sm text-gray-600">
              Add, edit, or archive fields that appear on records.
            </p>
          </div>

          <div className="space-y-3">
            {activeFields.map((field) => (
              <div
                key={field.id}
                className="rounded-xl border border-gray-200 bg-gray-50 p-4"
              >
                <div className="flex flex-wrap items-center justify-between gap-3">
                  <div>
                    <p className="text-sm font-semibold text-gray-900">
                      {field.label}
                    </p>
                    <p className="text-xs text-gray-500">
                      {typeLabel[field.type]}
                      {field.required ? " • Required" : ""}
                    </p>
                    {field.type === "select" && field.options?.length ? (
                      <p className="text-xs text-gray-500">
                        Options: {field.options.join(", ")}
                      </p>
                    ) : null}
                    {field.type === "multi_select" && field.options?.length ? (
                      <p className="text-xs text-gray-500">
                        Multi-select options: {field.options.join(", ")}
                      </p>
                    ) : null}
                    {field.enforceOptions ? (
                      <p className="text-xs text-gray-500">
                        Option enforcement enabled.
                      </p>
                    ) : null}
                  </div>
                  <div className="flex flex-wrap items-center gap-2 text-xs font-semibold">
                    <button
                      type="button"
                      onClick={() => handleEditField(field)}
                      className="rounded-full border border-gray-200 px-3 py-1 text-gray-600 transition hover:border-[#62ac4a] hover:text-[#62ac4a]"
                    >
                      Edit
                    </button>
                    <button
                      type="button"
                      onClick={() => handleArchiveField(field.id)}
                      className="rounded-full border border-gray-200 px-3 py-1 text-gray-600 transition hover:border-red-400 hover:text-red-600"
                    >
                      Archive
                    </button>
                  </div>
                </div>
                <div className="mt-3">
                  <label className="text-xs font-semibold uppercase tracking-wider text-[#62ac4a]">
                    Section assignment
                  </label>
                  <select
                    className="mt-2 w-full rounded-lg border border-gray-200 bg-white px-3 py-2 text-sm text-gray-700 focus:border-[#62ac4a] focus:outline-none focus:ring-1 focus:ring-[#62ac4a]"
                    value={
                      sections.find((section) =>
                        section.fieldIds.includes(field.id)
                      )?.id ?? ""
                    }
                    onChange={(event) =>
                      handleAssignField(field.id, event.target.value)
                    }
                  >
                    <option value="" disabled>
                      Select section
                    </option>
                    {sections.map((section) => (
                      <option key={section.id} value={section.id}>
                        {section.name}
                      </option>
                    ))}
                  </select>
                </div>
              </div>
            ))}
          </div>

          <div className="rounded-xl border border-dashed border-gray-300 p-4">
            <h5 className="text-sm font-semibold text-gray-900">
              Add a new field
            </h5>
            <div className="mt-3 grid gap-3">
              <input
                value={draftField.label}
                onChange={(event) =>
                  setDraftField((current) => ({
                    ...current,
                    label: event.target.value,
                  }))
                }
                placeholder="Field label"
                className="w-full rounded-lg border border-gray-200 bg-white px-3 py-2 text-sm text-gray-700 placeholder:text-gray-400 focus:border-[#62ac4a] focus:outline-none focus:ring-1 focus:ring-[#62ac4a]"
              />
              <p className="text-xs text-gray-500">
                Field keys are generated automatically from the label.
              </p>
              <div className="grid gap-3 md:grid-cols-2">
                <select
                  value={draftField.type}
                  onChange={(event) =>
                    setDraftField((current) => ({
                      ...current,
                      type: event.target.value as FieldType,
                    }))
                  }
                  aria-label="Field type"
                  className="w-full rounded-lg border border-gray-200 bg-white px-3 py-2 text-sm text-gray-700 focus:border-[#62ac4a] focus:outline-none focus:ring-1 focus:ring-[#62ac4a]"
                >
                  {fieldTypes.map((type) => (
                    <option key={type} value={type}>
                      {typeLabel[type]}
                    </option>
                  ))}
                </select>

                <label className="flex items-center gap-2 text-sm text-gray-600">
                  <input
                    type="checkbox"
                    checked={draftField.required}
                    onChange={(event) =>
                      setDraftField((current) => ({
                        ...current,
                        required: event.target.checked,
                      }))
                    }
                    className="h-4 w-4 rounded border-gray-300 text-[#62ac4a] focus:ring-[#62ac4a]"
                  />
                  Required field
                </label>
              </div>
              {draftField.type === "select" ? (
                <div className="space-y-1">
                  <input
                    value={draftField.options}
                    onChange={(event) =>
                      setDraftField((current) => ({
                        ...current,
                        options: event.target.value,
                      }))
                    }
                    placeholder="List options"
                    className="w-full rounded-lg border border-gray-200 bg-white px-3 py-2 text-sm text-gray-700 placeholder:text-gray-400 focus:border-[#62ac4a] focus:outline-none focus:ring-1 focus:ring-[#62ac4a]"
                  />
                  <p className="text-xs text-gray-500">
                    Example: Option 1, Option 2, Option 3.
                  </p>
                </div>
              ) : null}
              {draftField.type === "multi_select" ? (
                <div className="space-y-1">
                  <input
                    value={draftField.options}
                    onChange={(event) =>
                      setDraftField((current) => ({
                        ...current,
                        options: event.target.value,
                      }))
                    }
                    placeholder="List options"
                    className="w-full rounded-lg border border-gray-200 bg-white px-3 py-2 text-sm text-gray-700 placeholder:text-gray-400 focus:border-[#62ac4a] focus:outline-none focus:ring-1 focus:ring-[#62ac4a]"
                  />
                  <p className="text-xs text-gray-500">
                    Example: Option 1, Option 2, Option 3.
                  </p>
                </div>
              ) : null}
              {fieldTypesWithOptions.has(draftField.type) ? (
                <label className="flex items-center gap-2 text-sm text-gray-600">
                  <input
                    type="checkbox"
                    checked={draftField.enforceOptions}
                    onChange={(event) =>
                      setDraftField((current) => ({
                        ...current,
                        enforceOptions: event.target.checked,
                      }))
                    }
                    className="h-4 w-4 rounded border-gray-300 text-[#62ac4a] focus:ring-[#62ac4a]"
                  />
                  Enforce option list
                </label>
              ) : null}
              {draftError ? (
                <p className="text-xs text-red-600">{draftError}</p>
              ) : null}
              <button
                type="button"
                onClick={handleAddField}
                className="w-full rounded-full border border-[#62ac4a] bg-[#e8f5e9] px-4 py-2 text-sm font-semibold text-[#41734a] transition hover:bg-[#62ac4a] hover:text-white"
              >
                Add field
              </button>
            </div>
          </div>

          {editingFieldId && editDraft ? (
            <div className="rounded-xl border border-gray-200 bg-gray-50 p-4">
              <h5 className="text-sm font-semibold text-gray-900">
                Edit field
              </h5>
              <div className="mt-3 grid gap-3">
                <input
                  value={editDraft.label}
                  onChange={(event) =>
                    setEditDraft((current) =>
                      current
                        ? { ...current, label: event.target.value }
                        : current
                    )
                  }
                  placeholder="Field label"
                  className="w-full rounded-lg border border-gray-200 bg-white px-3 py-2 text-sm text-gray-700 placeholder:text-gray-400 focus:border-[#62ac4a] focus:outline-none focus:ring-1 focus:ring-[#62ac4a]"
                />
                <p className="text-xs text-gray-500">
                  Field keys are generated automatically from the label.
                </p>
                <div className="grid gap-3 md:grid-cols-2">
                  <select
                    value={editDraft.type}
                    onChange={(event) =>
                      setEditDraft((current) =>
                        current
                          ? {
                              ...current,
                              type: event.target.value as FieldType,
                            }
                          : current
                      )
                    }
                    aria-label="Field type"
                    className="w-full rounded-lg border border-gray-200 bg-white px-3 py-2 text-sm text-gray-700 focus:border-[#62ac4a] focus:outline-none focus:ring-1 focus:ring-[#62ac4a]"
                  >
                    {fieldTypes.map((type) => (
                      <option key={type} value={type}>
                        {typeLabel[type]}
                      </option>
                    ))}
                  </select>
                  <label className="flex items-center gap-2 text-sm text-gray-600">
                    <input
                      type="checkbox"
                      checked={editDraft.required}
                      onChange={(event) =>
                        setEditDraft((current) =>
                          current
                            ? { ...current, required: event.target.checked }
                            : current
                        )
                      }
                      className="h-4 w-4 rounded border-gray-300 text-[#62ac4a] focus:ring-[#62ac4a]"
                    />
                    Required field
                  </label>
                </div>
                {editDraft.type === "select" ? (
                  <div className="space-y-1">
                    <input
                      value={editDraft.options}
                      onChange={(event) =>
                        setEditDraft((current) =>
                          current
                            ? { ...current, options: event.target.value }
                            : current,
                        )
                      }
                      placeholder="List options"
                      className="w-full rounded-lg border border-gray-200 bg-white px-3 py-2 text-sm text-gray-700 placeholder:text-gray-400 focus:border-[#62ac4a] focus:outline-none focus:ring-1 focus:ring-[#62ac4a]"
                    />
                    <p className="text-xs text-gray-500">
                      Example: Option 1, Option 2, Option 3.
                    </p>
                  </div>
                ) : null}
                {editDraft.type === "multi_select" ? (
                  <div className="space-y-1">
                    <input
                      value={editDraft.options}
                      onChange={(event) =>
                        setEditDraft((current) =>
                          current
                            ? { ...current, options: event.target.value }
                            : current,
                        )
                      }
                      placeholder="List options"
                      className="w-full rounded-lg border border-gray-200 bg-white px-3 py-2 text-sm text-gray-700 placeholder:text-gray-400 focus:border-[#62ac4a] focus:outline-none focus:ring-1 focus:ring-[#62ac4a]"
                    />
                    <p className="text-xs text-gray-500">
                      Example: Option 1, Option 2, Option 3.
                    </p>
                  </div>
                ) : null}
                {fieldTypesWithOptions.has(editDraft.type) ? (
                  <label className="flex items-center gap-2 text-sm text-gray-600">
                    <input
                      type="checkbox"
                      checked={editDraft.enforceOptions}
                      onChange={(event) =>
                        setEditDraft((current) =>
                          current
                            ? {
                                ...current,
                                enforceOptions: event.target.checked,
                              }
                            : current
                        )
                      }
                      className="h-4 w-4 rounded border-gray-300 text-[#62ac4a] focus:ring-[#62ac4a]"
                    />
                    Enforce option list
                  </label>
                ) : null}
                {editError ? (
                  <p className="text-xs text-red-600">{editError}</p>
                ) : null}
                <div className="flex flex-wrap items-center gap-2">
                  <button
                    type="button"
                    onClick={handleSaveEdit}
                    className="rounded-full border border-[#62ac4a] bg-[#e8f5e9] px-4 py-2 text-sm font-semibold text-[#41734a] transition hover:bg-[#62ac4a] hover:text-white"
                  >
                    Save changes
                  </button>
                  <button
                    type="button"
                    onClick={handleCancelEdit}
                    className="rounded-full border border-gray-200 px-4 py-2 text-sm font-semibold text-gray-600 transition hover:border-[#62ac4a] hover:text-[#62ac4a]"
                  >
                    Cancel
                  </button>
                </div>
              </div>
            </div>
          ) : null}

          {archivedFields.length > 0 ? (
            <div className="rounded-xl border border-gray-200 bg-gray-50 p-4">
              <h5 className="text-sm font-semibold text-gray-900">
                Archived fields
              </h5>
              <div className="mt-3 space-y-2 text-sm text-gray-600">
                {archivedFields.map((field) => (
                  <div key={field.id}>
                    {field.label} • {typeLabel[field.type]}
                  </div>
                ))}
              </div>
            </div>
          ) : null}
        </div>

        <div className="space-y-4 rounded-xl border border-gray-200 bg-white p-6">
          <div>
            <h4 className="text-sm font-semibold uppercase tracking-wider text-[#62ac4a]">
              Layout sections
            </h4>
            <p className="text-sm text-gray-600">
              Create sections and reorder fields within each section.
            </p>
          </div>

          <div className="rounded-xl border border-dashed border-gray-300 p-4">
            <h5 className="text-sm font-semibold text-gray-900">
              Add a new section
            </h5>
            <div className="mt-3 flex flex-wrap items-center gap-2">
              <input
                value={newSectionName}
                onChange={(event) => setNewSectionName(event.target.value)}
                placeholder="Section name"
                className="flex-1 rounded-lg border border-gray-200 bg-white px-3 py-2 text-sm text-gray-700 focus:border-[#62ac4a] focus:outline-none focus:ring-1 focus:ring-[#62ac4a]"
              />
              <button
                type="button"
                onClick={handleAddSection}
                className="rounded-full border border-[#62ac4a] bg-[#e8f5e9] px-4 py-2 text-sm font-semibold text-[#41734a] transition hover:bg-[#62ac4a] hover:text-white"
              >
                Add section
              </button>
            </div>
          </div>

          <SectionEditor
            sections={sections}
            setSections={setSections}
            fields={fields}
          />
        </div>
      </div>
    </section>
  );
}

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

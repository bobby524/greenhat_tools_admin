"use client";

import { useEffect, useState } from "react";
import { SectionEditor } from "./SectionEditor";
import {
  fieldTypes,
  getEntityStorageKey,
  loadEntitySettings,
  saveEntitySettings,
  saveStoredValue,
  type FieldDefinition,
  type FieldType,
  type SectionDefinition,
} from "../data/customization";

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

type FieldDraft = {
  label: string;
  type: FieldType;
  required: boolean;
  options: string;
  enforceOptions: boolean;
};

type EntitySettings = {
  id: string;
  title: string;
  description: string;
  fields: FieldDefinition[];
  sections: SectionDefinition[];
};

const createId = (prefix: string) =>
  `${prefix}-${globalThis.crypto?.randomUUID?.() ?? Math.random().toString(36).slice(2)}`;

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

export function EntitySettingsEditor({
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
          : section,
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
        field.id === fieldId ? { ...field, archived: true } : field,
      ),
    );
    setSections((current) =>
      current.map((section) => ({
        ...section,
        fieldIds: section.fieldIds.filter((id) => id !== fieldId),
      })),
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
      }),
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
          : field,
      ),
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
                        section.fieldIds.includes(field.id),
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
                        : current,
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
                          : current,
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
                            : current,
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
                            : current,
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

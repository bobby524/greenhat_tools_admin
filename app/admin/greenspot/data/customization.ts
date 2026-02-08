import { getSupabaseClient } from "./supabaseClient";
import type { Database, Json } from "./types";

export const crmSettingsStoragePrefix = "crm-settings";

export const getEntityStorageKey = (entityId: string) =>
  `${crmSettingsStoragePrefix}-entity-${entityId}`;

export const dealStagesStorageKey = `${crmSettingsStoragePrefix}-deal-stages`;
export const dealPipelinesStorageKey = `${crmSettingsStoragePrefix}-deal-pipelines`;
export const selectedPipelineStorageKey = `${crmSettingsStoragePrefix}-selected-pipeline`;

const entityObjectTypes = {
  contacts: "crm_contacts",
  companies: "crm_companies",
  deals: "crm_deals",
  tasks: "crm_tasks",
  activities: "crm_activities",
} as const;

type EntityId = keyof typeof entityObjectTypes;
type ObjectType = (typeof entityObjectTypes)[EntityId];

type FieldDefinitionRow =
  Database["public"]["Tables"]["crm_field_definitions"]["Row"];
type FieldOptionRow =
  Database["public"]["Tables"]["crm_field_options"]["Row"];
type LayoutSectionRow =
  Database["public"]["Tables"]["crm_layout_sections"]["Row"];
type LayoutFieldRow =
  Database["public"]["Tables"]["crm_layout_fields"]["Row"];

type SupabaseCustomizationPayload = {
  fields: FieldDefinitionRow[];
  options: FieldOptionRow[];
  sections: LayoutSectionRow[];
  layoutFields: LayoutFieldRow[];
};

const isSupportedEntity = (entityId: string): entityId is EntityId =>
  entityId in entityObjectTypes;

const toObjectType = (entityId: string): ObjectType | null =>
  isSupportedEntity(entityId) ? entityObjectTypes[entityId] : null;

const isJsonObject = (value: Json | null): value is Record<string, Json> =>
  typeof value === "object" && value !== null && !Array.isArray(value);

const getArchivedFlag = (archivedAt: string | null) => archivedAt !== null;

const toOptionKey = (label: string, index: number) => {
  const normalized = label
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "_")
    .replace(/^_+|_+$/g, "");

  if (!normalized) {
    return `option_${index + 1}`;
  }

  return `${normalized}_${index + 1}`;
};

const extractOptions = (defaultValue: Json | null) => {
  if (!isJsonObject(defaultValue)) {
    return undefined;
  }
  const options = defaultValue.options;
  if (!Array.isArray(options)) {
    return undefined;
  }

  const sanitized = options.filter(
    (option): option is string =>
      typeof option === "string" && option.trim().length > 0,
  );

  return sanitized.length > 0 ? sanitized : undefined;
};

export const loadStoredValue = <T,>(key: string): T | null => {
  if (typeof window === "undefined") {
    return null;
  }
  try {
    const raw = window.localStorage.getItem(key);
    if (!raw) {
      return null;
    }
    return JSON.parse(raw) as T;
  } catch (error) {
    console.error("Failed to load CRM settings from storage", error);
    return null;
  }
};

export const saveStoredValue = (key: string, value: unknown) => {
  if (typeof window === "undefined") {
    return;
  }
  try {
    window.localStorage.setItem(key, JSON.stringify(value));
  } catch (error) {
    console.error("Failed to save CRM settings to storage", error);
  }
};

export const fieldTypes = [
  "text",
  "number",
  "date",
  "boolean",
  "select",
  "multi_select",
  "user",
  "url",
] as const;

export type FieldType = (typeof fieldTypes)[number];

export type FieldDefinition = {
  id: string;
  label: string;
  fieldKey: string;
  type: FieldType;
  required: boolean;
  options?: string[];
  enforceOptions?: boolean;
  archived?: boolean;
};

export type SectionDefinition = {
  id: string;
  name: string;
  fieldIds: string[];
};

export type PersistedEntitySettings = {
  fields: FieldDefinition[];
  sections: SectionDefinition[];
};

const isFieldType = (value: unknown): value is FieldType =>
  typeof value === "string" &&
  (fieldTypes as readonly string[]).includes(value);

const sanitizeField = (value: unknown): FieldDefinition | null => {
  if (!value || typeof value !== "object") {
    return null;
  }

  const record = value as Record<string, unknown>;
  const id = typeof record.id === "string" ? record.id : null;
  const label = typeof record.label === "string" ? record.label : null;
  const fieldKey =
    typeof record.fieldKey === "string" ? record.fieldKey : null;
  const type = isFieldType(record.type) ? record.type : null;
  const required = Boolean(record.required);
  const archived = Boolean(record.archived);
  const options = Array.isArray(record.options)
    ? record.options.filter(
        (option): option is string =>
          typeof option === "string" && option.trim().length > 0,
      )
    : undefined;

  if (!id || !label || !fieldKey || !type) {
    return null;
  }

  return {
    id,
    label,
    fieldKey,
    type,
    required,
    options,
    enforceOptions: Boolean(record.enforceOptions),
    archived,
  } satisfies FieldDefinition;
};

const sanitizeSection = (value: unknown): SectionDefinition | null => {
  if (!value || typeof value !== "object") {
    return null;
  }
  const record = value as Record<string, unknown>;
  const id = typeof record.id === "string" ? record.id : null;
  const name = typeof record.name === "string" ? record.name : null;
  const fieldIds = Array.isArray(record.fieldIds)
    ? record.fieldIds.filter(
        (fieldId): fieldId is string =>
          typeof fieldId === "string" && fieldId.trim().length > 0,
      )
    : [];

  if (!id || !name) {
    return null;
  }

  return {
    id,
    name,
    fieldIds,
  } satisfies SectionDefinition;
};

const loadStoredEntitySettings = (
  entityId: string,
): PersistedEntitySettings | null => {
  const stored = loadStoredValue<PersistedEntitySettings>(
    getEntityStorageKey(entityId),
  );
  if (!stored) {
    return null;
  }

  const fields = Array.isArray(stored.fields)
    ? stored.fields
        .map((field) => sanitizeField(field))
        .filter((field): field is FieldDefinition => Boolean(field))
    : [];
  const sections = Array.isArray(stored.sections)
    ? stored.sections
        .map((section) => sanitizeSection(section))
        .filter((section): section is SectionDefinition => Boolean(section))
    : [];

  if (fields.length === 0 || sections.length === 0) {
    return null;
  }

  return { fields, sections } satisfies PersistedEntitySettings;
};

const loadEntitySettingsFromSupabase = async (
  objectType: ObjectType,
): Promise<PersistedEntitySettings | null> => {
  const supabase = getSupabaseClient();

  const [fieldsResult, sectionsResult] = await Promise.all([
    supabase
      .from("crm_field_definitions")
      .select("*")
      .eq("object_type", objectType)
      .order("created_at", { ascending: true }),
    supabase
      .from("crm_layout_sections")
      .select("*")
      .eq("object_type", objectType)
      .order("display_order", { ascending: true }),
  ]);

  if (fieldsResult.error || sectionsResult.error) {
    console.error("Failed to load CRM customization settings", {
      fieldsError: fieldsResult.error,
      sectionsError: sectionsResult.error,
    });
    return null;
  }

  const fields = (fieldsResult.data ?? []) as FieldDefinitionRow[];
  const sections = (sectionsResult.data ?? []) as LayoutSectionRow[];

  if (fields.length === 0 || sections.length === 0) {
    return null;
  }

  const fieldIds = fields.map((field) => field.id);
  const sectionIds = sections.map((section) => section.id);

  const [optionsResult, layoutFieldsResult] = await Promise.all([
    supabase
      .from("crm_field_options")
      .select("*")
      .in("field_definition_id", fieldIds)
      .order("display_order", { ascending: true }),
    supabase
      .from("crm_layout_fields")
      .select("*")
      .in("section_id", sectionIds)
      .order("display_order", { ascending: true }),
  ]);

  if (optionsResult.error || layoutFieldsResult.error) {
    console.error("Failed to load CRM customization settings", {
      optionsError: optionsResult.error,
      layoutFieldsError: layoutFieldsResult.error,
    });
    return null;
  }

  const payload: SupabaseCustomizationPayload = {
    fields,
    options: (optionsResult.data ?? []) as FieldOptionRow[],
    sections,
    layoutFields: (layoutFieldsResult.data ?? []) as LayoutFieldRow[],
  };

  const optionsByFieldId = payload.options.reduce<Record<string, string[]>>(
    (accumulator, option) => {
      const list = accumulator[option.field_definition_id] ?? [];
      list.push(option.label);
      accumulator[option.field_definition_id] = list;
      return accumulator;
    },
    {},
  );

  const fieldsById = payload.fields.reduce<Record<string, FieldDefinition>>(
    (accumulator, field) => {
      const fallbackOptions = extractOptions(field.default_value);
      accumulator[field.id] = {
        id: field.id,
        label: field.label,
        fieldKey: field.field_key,
        type: isFieldType(field.field_type) ? field.field_type : "text",
        required: field.required,
        options: optionsByFieldId[field.id] ?? fallbackOptions,
        enforceOptions: true,
        archived: getArchivedFlag(field.archived_at),
      } satisfies FieldDefinition;
      return accumulator;
    },
    {},
  );

  const layoutFieldsBySection = payload.layoutFields.reduce<
    Record<string, LayoutFieldRow[]>
  >((accumulator, layoutField) => {
    const list = accumulator[layoutField.section_id] ?? [];
    list.push(layoutField);
    accumulator[layoutField.section_id] = list;
    return accumulator;
  }, {});

  const mappedSections = payload.sections.map((section) => {
    const layoutFields = layoutFieldsBySection[section.id] ?? [];
    const fieldIds = layoutFields
      .map((layoutField) => layoutField.field_definition_id)
      .filter((fieldId) => fieldsById[fieldId] !== undefined);

    return {
      id: section.id,
      name: section.section_name,
      fieldIds,
    } satisfies SectionDefinition;
  });

  const mappedFields = Object.values(fieldsById);

  const result = {
    fields: mappedFields,
    sections: mappedSections,
  } satisfies PersistedEntitySettings;

  return result.fields.length > 0 && result.sections.length > 0 ? result : null;
};

const deleteExistingCustomization = async (objectType: ObjectType) => {
  const supabase = getSupabaseClient();

  const existingSectionsResult = await supabase
    .from("crm_layout_sections")
    .select("id")
    .eq("object_type", objectType);

  if (existingSectionsResult.error) {
    return existingSectionsResult.error;
  }

  const existingSectionIds = (existingSectionsResult.data ?? []).map(
    (section) => section.id,
  );

  if (existingSectionIds.length > 0) {
    const layoutDeleteResult = await supabase
      .from("crm_layout_fields")
      .delete()
      .in("section_id", existingSectionIds);
    if (layoutDeleteResult.error) {
      return layoutDeleteResult.error;
    }
  }

  const sectionsDeleteResult = await supabase
    .from("crm_layout_sections")
    .delete()
    .eq("object_type", objectType);
  if (sectionsDeleteResult.error) {
    return sectionsDeleteResult.error;
  }

  const existingFieldsResult = await supabase
    .from("crm_field_definitions")
    .select("id")
    .eq("object_type", objectType);

  if (existingFieldsResult.error) {
    return existingFieldsResult.error;
  }

  const existingFieldIds = (existingFieldsResult.data ?? []).map(
    (field) => field.id,
  );

  if (existingFieldIds.length > 0) {
    const optionsDeleteResult = await supabase
      .from("crm_field_options")
      .delete()
      .in("field_definition_id", existingFieldIds);
    if (optionsDeleteResult.error) {
      return optionsDeleteResult.error;
    }
  }

  const fieldsDeleteResult = await supabase
    .from("crm_field_definitions")
    .delete()
    .eq("object_type", objectType);

  return fieldsDeleteResult.error ?? null;
};

const insertCustomization = async (
  objectType: ObjectType,
  settings: PersistedEntitySettings,
) => {
  const supabase = getSupabaseClient();

  const fieldPayload = settings.fields.map((field) => {
    const options = field.options ?? [];
    const defaultValue =
      options.length > 0 ? ({ options } satisfies Json) : null;

    return {
      object_type: objectType,
      field_key: field.fieldKey,
      label: field.label,
      field_type: field.type,
      required: field.required,
      default_value: defaultValue,
      archived_at: field.archived ? new Date().toISOString() : null,
    } satisfies Database["public"]["Tables"]["crm_field_definitions"]["Insert"];
  });

  const fieldsInsertResult = await supabase
    .from("crm_field_definitions")
    .insert(fieldPayload)
    .select("*");

  if (fieldsInsertResult.error) {
    return { error: fieldsInsertResult.error } as const;
  }

  const insertedFields = (fieldsInsertResult.data ?? []) as FieldDefinitionRow[];

  const fieldIdByKey = insertedFields.reduce<Record<string, string>>(
    (accumulator, field) => {
      accumulator[field.field_key] = field.id;
      return accumulator;
    },
    {},
  );

  const optionsPayload = settings.fields.flatMap((field) => {
    const fieldId = fieldIdByKey[field.fieldKey];
    if (!fieldId || !field.options) {
      return [];
    }
    return field.options.map((option, index) => ({
      field_definition_id: fieldId,
      option_key: toOptionKey(option, index),
      label: option,
      display_order: index,
      archived_at: field.archived ? new Date().toISOString() : null,
    })) satisfies Database["public"]["Tables"]["crm_field_options"]["Insert"][];
  });

  if (optionsPayload.length > 0) {
    const optionsInsertResult = await supabase
      .from("crm_field_options")
      .insert(optionsPayload);
    if (optionsInsertResult.error) {
      return { error: optionsInsertResult.error } as const;
    }
  }

  const sectionsPayload = settings.sections.map((section, index) => ({
    object_type: objectType,
    section_name: section.name,
    display_order: index,
    archived_at: null,
  })) satisfies Database["public"]["Tables"]["crm_layout_sections"]["Insert"][];

  const sectionsInsertResult = await supabase
    .from("crm_layout_sections")
    .insert(sectionsPayload)
    .select("*");

  if (sectionsInsertResult.error) {
    return { error: sectionsInsertResult.error } as const;
  }

  const insertedSections = (sectionsInsertResult.data ?? []) as LayoutSectionRow[];

  const sectionsByName = insertedSections.reduce<Record<string, string>>(
    (accumulator, section) => {
      accumulator[section.section_name] = section.id;
      return accumulator;
    },
    {},
  );

  const fieldById = settings.fields.reduce<Record<string, FieldDefinition>>(
    (accumulator, field) => {
      accumulator[field.id] = field;
      return accumulator;
    },
    {},
  );

  const layoutFieldsPayload = settings.sections.flatMap((section) => {
    const sectionId = sectionsByName[section.name];
    if (!sectionId) {
      return [];
    }

    const fieldIds = section.fieldIds
      .map((fieldId) => fieldById[fieldId])
      .filter((field): field is FieldDefinition => Boolean(field))
      .map((field) => fieldIdByKey[field.fieldKey])
      .filter((fieldId): fieldId is string => Boolean(fieldId));

    return fieldIds.map((fieldId, index) => ({
      section_id: sectionId,
      field_definition_id: fieldId,
      display_order: index,
      archived_at: null,
    })) satisfies Database["public"]["Tables"]["crm_layout_fields"]["Insert"][];
  });

  if (layoutFieldsPayload.length > 0) {
    const layoutInsertResult = await supabase
      .from("crm_layout_fields")
      .insert(layoutFieldsPayload);
    if (layoutInsertResult.error) {
      return { error: layoutInsertResult.error } as const;
    }
  }

  const mappedFields = settings.fields.map((field) => {
    const insertedId = fieldIdByKey[field.fieldKey];
    return insertedId ? { ...field, id: insertedId } : field;
  });

  const mappedSections = settings.sections.map((section) => ({
    ...section,
    id: sectionsByName[section.name] ?? section.id,
    fieldIds: section.fieldIds
      .map((fieldId) => fieldById[fieldId])
      .filter((field): field is FieldDefinition => Boolean(field))
      .map((field) => fieldIdByKey[field.fieldKey] ?? field.id),
  }));

  return {
    error: null,
    data: {
      fields: mappedFields,
      sections: mappedSections,
    } satisfies PersistedEntitySettings,
  } as const;
};

export const loadEntitySettings = async (
  entityId: string,
): Promise<PersistedEntitySettings | null> => {
  const objectType = toObjectType(entityId);
  if (!objectType) {
    return loadStoredEntitySettings(entityId);
  }

  const supabaseSettings = await loadEntitySettingsFromSupabase(objectType);
  if (supabaseSettings) {
    saveStoredValue(getEntityStorageKey(entityId), supabaseSettings);
    return supabaseSettings;
  }

  return loadStoredEntitySettings(entityId);
};

export const saveEntitySettings = async (
  entityId: string,
  settings: PersistedEntitySettings,
) => {
  const objectType = toObjectType(entityId);
  if (!objectType) {
    saveStoredValue(getEntityStorageKey(entityId), settings);
    return { error: null, data: settings } as const;
  }

  const deleteError = await deleteExistingCustomization(objectType);
  if (deleteError) {
    console.error("Failed to clear CRM customization settings", deleteError);
    return { error: deleteError, data: settings } as const;
  }

  const insertResult = await insertCustomization(objectType, settings);
  if (insertResult.error || !insertResult.data) {
    console.error("Failed to persist CRM customization settings", insertResult.error);
    return { error: insertResult.error, data: settings } as const;
  }

  saveStoredValue(getEntityStorageKey(entityId), insertResult.data);
  return insertResult;
};

export const initialContactFields: FieldDefinition[] = [
  {
    id: "contact-owner",
    label: "Account owner",
    fieldKey: "owner",
    type: "text",
    required: false,
  },
  {
    id: "contact-first-name",
    label: "First name",
    fieldKey: "first_name",
    type: "text",
    required: true,
  },
  {
    id: "contact-last-name",
    label: "Last name",
    fieldKey: "last_name",
    type: "text",
    required: true,
  },
  {
    id: "contact-email",
    label: "Email",
    fieldKey: "email",
    type: "text",
    required: true,
  },
  {
    id: "contact-lifecycle",
    label: "Lifecycle stage",
    fieldKey: "lifecycle_stage",
    type: "select",
    required: false,
    options: ["Subscriber", "Lead", "Customer"],
  },
  {
    id: "contact-birthday",
    label: "Birthday",
    fieldKey: "birthday",
    type: "date",
    required: false,
  },
  {
    id: "contact-last-contacted",
    label: "Last contacted",
    fieldKey: "last_contacted",
    type: "date",
    required: false,
  },
  {
    id: "contact-next-touchpoint",
    label: "Next touchpoint",
    fieldKey: "next_touchpoint",
    type: "date",
    required: false,
  },
  {
    id: "contact-preferred-channel",
    label: "Preferred channel",
    fieldKey: "preferredChannel",
    type: "select",
    required: false,
    options: ["Email", "Phone", "SMS", "LinkedIn"],
  },
  {
    id: "contact-priority-score",
    label: "Priority score",
    fieldKey: "priorityScore",
    type: "number",
    required: false,
  },
  {
    id: "contact-newsletter-opt-in",
    label: "Newsletter opt-in",
    fieldKey: "newsletterOptIn",
    type: "boolean",
    required: false,
  },
  {
    id: "contact-time-zone",
    label: "Time zone",
    fieldKey: "timeZone",
    type: "text",
    required: false,
  },
  {
    id: "contact-website",
    label: "Website",
    fieldKey: "website",
    type: "url",
    required: false,
  },
  {
    id: "contact-profile-summary",
    label: "Profile summary",
    fieldKey: "summary",
    type: "text",
    required: false,
  },
];

export const normalizeFieldKey = (label: string): string => {
  const normalized = label
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "_")
    .replace(/^_+|_+$/g, "");

  return normalized || "field";
};

export const createUniqueFieldKey = (
  label: string,
  existingFields: Array<{ fieldKey: string }>
): string => {
  const baseKey = normalizeFieldKey(label);
  const existingKeys = new Set(
    existingFields.map((f) => f.fieldKey.toLowerCase())
  );

  if (!existingKeys.has(baseKey.toLowerCase())) {
    return baseKey;
  }

  let counter = 2;
  let newKey = `${baseKey}_${counter}`;
  while (existingKeys.has(newKey.toLowerCase())) {
    counter++;
    newKey = `${baseKey}_${counter}`;
  }

  return newKey;
};

export const initialContactSections: SectionDefinition[] = [
  {
    id: "contact-overview",
    name: "Overview",
    fieldIds: [
      "contact-first-name",
      "contact-last-name",
      "contact-email",
      "contact-owner",
    ],
  },
  {
    id: "contact-details",
    name: "Details",
    fieldIds: ["contact-lifecycle", "contact-birthday"],
  },
  {
    id: "contact-engagement",
    name: "Engagement",
    fieldIds: [
      "contact-last-contacted",
      "contact-next-touchpoint",
      "contact-preferred-channel",
      "contact-priority-score",
      "contact-newsletter-opt-in",
    ],
  },
  {
    id: "contact-profile-notes",
    name: "Profile notes",
    fieldIds: [
      "contact-time-zone",
      "contact-website",
      "contact-profile-summary",
    ],
  },
];

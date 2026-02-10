"use client";

import { useMemo } from "react";
import type { FieldDefinition, SectionDefinition } from "../data/customization";

const typeLabel: Record<string, string> = {
  text: "Text",
  number: "Number",
  date: "Date",
  boolean: "Boolean",
  select: "Select",
  multi_select: "Multi-select",
  user: "User",
  url: "URL",
};

interface SectionEditorProps {
  sections: SectionDefinition[];
  setSections: React.Dispatch<React.SetStateAction<SectionDefinition[]>>;
  fields: FieldDefinition[];
}

export function SectionEditor({
  sections,
  setSections,
  fields,
}: SectionEditorProps) {
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

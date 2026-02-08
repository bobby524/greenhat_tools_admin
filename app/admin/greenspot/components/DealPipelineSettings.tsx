"use client";

import { useEffect, useMemo, useState } from "react";
import CrmInlineError from "./CrmInlineError";
import {
  defaultDealPipelines,
  loadDealPipelines,
  loadStoredPipelineSelection,
  saveDealPipelines,
  saveStoredPipelineSelection,
  type DealPipeline,
  type DealPipelineStage,
} from "../data/pipelineData";

type DealStage = DealPipelineStage & {
  dealsCount: number;
};

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

const createId = (prefix: string) =>
  `${prefix}-${globalThis.crypto?.randomUUID?.() ?? Math.random().toString(36).slice(2)}`;

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

export function DealPipelineSettings() {
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
    checked: boolean,
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
      }),
    );
  };

  const handleToggleActive = (stageKey: string) => {
    updatePipelineStages((current) =>
      current.map((stage) =>
        stage.key === stageKey ? { ...stage, isActive: !stage.isActive } : stage,
      ),
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
        normalizeText(stage.label) === normalizeText(trimmed),
    );
    if (isDuplicate) {
      setEditError("Stage labels must be unique.");
      return;
    }
    updatePipelineStages((current) =>
      current.map((stage) =>
        stage.key === editingKey ? { ...stage, label: trimmed } : stage,
      ),
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
      (stage) => normalizeText(stage.label) === normalizeText(label),
    );
    const keyConflict = stages.some(
      (stage) => normalizeText(stage.key) === normalizeText(key),
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
      ]),
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
                            event.target.checked,
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
                            event.target.checked,
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

import {
  dealPipelinesStorageKey,
  loadStoredValue,
  saveStoredValue,
  selectedPipelineStorageKey,
} from "./customization";
import { getSupabaseClient } from "./supabaseClient";
import type { Database } from "./types";

export type DealPipelineStage = {
  id: string;
  key: string;
  label: string;
  order: number;
  isClosedWon: boolean;
  isClosedLost: boolean;
  isActive: boolean;
};

export type DealPipeline = {
  id: string;
  name: string;
  description: string;
  stages: DealPipelineStage[];
};

type DealPipelineRow =
  Database["public"]["Tables"]["crm_deal_pipelines"]["Row"];
type DealPipelineStageRow =
  Database["public"]["Tables"]["crm_deal_pipeline_stages"]["Row"];
type DealStageRow = Database["public"]["Tables"]["crm_deal_stages"]["Row"];

const isMissingPipelineTables = (error: unknown) => {
  if (!error || typeof error !== "object") {
    return false;
  }
  const record = error as {
    code?: string;
    message?: string;
    details?: string;
    status?: number;
    statusCode?: number;
  };
  const message = record.message ?? "";
  const details = record.details ?? "";
  const status = record.status ?? record.statusCode ?? 0;
  return (
    status === 404 ||
    record.code === "42P01" ||
    record.code === "PGRST116" ||
    message.includes("crm_deal_pipelines") ||
    message.includes("crm_deal_pipeline_stages") ||
    message.includes("schema cache") ||
    details.includes("crm_deal_pipelines") ||
    details.includes("crm_deal_pipeline_stages")
  );
};

const defaultPipelineStages: DealPipelineStage[] = [
  {
    id: "stage-new",
    key: "new",
    label: "New",
    order: 1,
    isClosedWon: false,
    isClosedLost: false,
    isActive: true,
  },
  {
    id: "stage-qualified",
    key: "qualified",
    label: "Qualified",
    order: 2,
    isClosedWon: false,
    isClosedLost: false,
    isActive: true,
  },
  {
    id: "stage-proposal",
    key: "proposal",
    label: "Proposal",
    order: 3,
    isClosedWon: false,
    isClosedLost: false,
    isActive: true,
  },
  {
    id: "stage-negotiation",
    key: "negotiation",
    label: "Negotiation",
    order: 4,
    isClosedWon: false,
    isClosedLost: false,
    isActive: true,
  },
  {
    id: "stage-contract",
    key: "contract",
    label: "Contract",
    order: 5,
    isClosedWon: false,
    isClosedLost: false,
    isActive: true,
  },
  {
    id: "stage-closed-won",
    key: "closed_won",
    label: "Closed won",
    order: 6,
    isClosedWon: true,
    isClosedLost: false,
    isActive: true,
  },
  {
    id: "stage-closed-lost",
    key: "closed_lost",
    label: "Closed lost",
    order: 7,
    isClosedWon: false,
    isClosedLost: true,
    isActive: true,
  },
];

const renewalPipelineStages: DealPipelineStage[] = [
  {
    id: "stage-renewal-intake",
    key: "renewal_intake",
    label: "Renewal intake",
    order: 1,
    isClosedWon: false,
    isClosedLost: false,
    isActive: true,
  },
  {
    id: "stage-customer-review",
    key: "customer_review",
    label: "Customer review",
    order: 2,
    isClosedWon: false,
    isClosedLost: false,
    isActive: true,
  },
  {
    id: "stage-legal-review",
    key: "legal_review",
    label: "Legal review",
    order: 3,
    isClosedWon: false,
    isClosedLost: false,
    isActive: true,
  },
  {
    id: "stage-final-approval",
    key: "final_approval",
    label: "Final approval",
    order: 4,
    isClosedWon: false,
    isClosedLost: false,
    isActive: true,
  },
  {
    id: "stage-renewal-closed-won",
    key: "closed_won",
    label: "Closed won",
    order: 5,
    isClosedWon: true,
    isClosedLost: false,
    isActive: true,
  },
  {
    id: "stage-renewal-closed-lost",
    key: "closed_lost",
    label: "Closed lost",
    order: 6,
    isClosedWon: false,
    isClosedLost: true,
    isActive: true,
  },
];

export const defaultDealPipelines: DealPipeline[] = [
  {
    id: "core-sales",
    name: "Core sales pipeline",
    description: "Standard mid-market sales process from new lead to close.",
    stages: defaultPipelineStages,
  },
  {
    id: "renewals",
    name: "Renewals pipeline",
    description: "Track renewal deals with stakeholder and contract reviews.",
    stages: renewalPipelineStages,
  },
];

const isPipelineStage = (value: unknown): value is DealPipelineStage => {
  if (!value || typeof value !== "object") {
    return false;
  }
  const record = value as Record<string, unknown>;
  return (
    typeof record.key === "string" &&
    typeof record.label === "string" &&
    typeof record.order === "number" &&
    typeof record.isClosedWon === "boolean" &&
    typeof record.isClosedLost === "boolean" &&
    typeof record.isActive === "boolean"
  );
};

const sanitizePipeline = (value: unknown): DealPipeline | null => {
  if (!value || typeof value !== "object") {
    return null;
  }
  const record = value as Record<string, unknown>;
  const id = typeof record.id === "string" ? record.id : null;
  const name = typeof record.name === "string" ? record.name : null;
  const description =
    typeof record.description === "string" ? record.description : "";
  const stages = Array.isArray(record.stages)
    ? record.stages.filter(isPipelineStage)
    : [];

  if (!id || !name || stages.length === 0) {
    return null;
  }

  return {
    id,
    name,
    description,
    stages,
  };
};

export const loadStoredDealPipelines = (): DealPipeline[] | null => {
  const stored = loadStoredValue<DealPipeline[]>(dealPipelinesStorageKey);
  if (!stored) {
    return null;
  }
  const pipelines = stored
    .map((pipeline) => sanitizePipeline(pipeline))
    .filter((pipeline): pipeline is DealPipeline => Boolean(pipeline));

  return pipelines.length > 0 ? pipelines : null;
};

export const saveStoredDealPipelines = (pipelines: DealPipeline[]) => {
  saveStoredValue(dealPipelinesStorageKey, pipelines);
};

const mapSupabasePipelines = (
  pipelines: DealPipelineRow[],
  stages: DealPipelineStageRow[],
) => {
  const stagesByPipeline = stages.reduce<Record<string, DealPipelineStage[]>>(
    (accumulator, stage) => {
      const list = accumulator[stage.pipeline_id] ?? [];
      list.push({
        id: stage.id,
        key: stage.stage_key,
        label: stage.label,
        order: stage.order,
        isClosedWon: stage.is_closed_won,
        isClosedLost: stage.is_closed_lost,
        isActive: stage.is_active,
      });
      accumulator[stage.pipeline_id] = list;
      return accumulator;
    },
    {},
  );

  return pipelines
    .map((pipeline) => {
      const pipelineStages = (stagesByPipeline[pipeline.id] ?? []).sort(
        (a, b) => a.order - b.order,
      );
      if (pipelineStages.length === 0) {
        return null;
      }
      return {
        id: pipeline.id,
        name: pipeline.name,
        description: pipeline.description ?? "",
        stages: pipelineStages,
      } satisfies DealPipeline;
    })
    .filter((pipeline): pipeline is DealPipeline => Boolean(pipeline));
};

const buildLegacyPipeline = (stages: DealStageRow[]): DealPipeline[] => {
  if (stages.length === 0) {
    return [];
  }

  const mappedStages = stages
    .map((stage) => ({
      id: `legacy-${stage.key}`,
      key: stage.key,
      label: stage.label,
      order: stage.order,
      isClosedWon: stage.is_closed_won,
      isClosedLost: stage.is_closed_lost,
      isActive: true,
    }))
    .sort((a, b) => a.order - b.order);

  return [
    {
      id: "core-sales",
      name: "Core sales pipeline",
      description: "Imported from crm_deal_stages.",
      stages: mappedStages,
    },
  ];
};

export const loadDealPipelines = async (): Promise<DealPipeline[] | null> => {
  const cached = loadStoredDealPipelines();
  try {
    const supabase = getSupabaseClient();
    const [pipelinesResult, stagesResult] = await Promise.all([
      supabase.from("crm_deal_pipelines").select("*").order("created_at"),
      supabase
        .from("crm_deal_pipeline_stages")
        .select("*")
        .order("order"),
    ]);

    if (pipelinesResult.error || stagesResult.error) {
      console.error("Failed to load CRM pipelines", {
        pipelinesError: pipelinesResult.error,
        stagesError: stagesResult.error,
      });
      if (
        isMissingPipelineTables(pipelinesResult.error) ||
        isMissingPipelineTables(stagesResult.error)
      ) {
        const legacyStagesResult = await supabase
          .from("crm_deal_stages")
          .select("*")
          .order("order");
        if (legacyStagesResult.error) {
          console.error("Failed to load legacy CRM deal stages", {
            legacyStagesError: legacyStagesResult.error,
          });
          saveStoredDealPipelines([]);
          return [];
        }

        const legacyPipelines = buildLegacyPipeline(
          (legacyStagesResult.data ?? []) as DealStageRow[],
        );
        saveStoredDealPipelines(legacyPipelines);
        return legacyPipelines;
      }
      return cached;
    }

    const pipelines = mapSupabasePipelines(
      (pipelinesResult.data ?? []) as DealPipelineRow[],
      (stagesResult.data ?? []) as DealPipelineStageRow[],
    );

    if (pipelines.length > 0) {
      saveStoredDealPipelines(pipelines);
      return pipelines;
    }

    saveStoredDealPipelines([]);
    return [];
  } catch (error) {
    console.error("Failed to load CRM pipelines", error);
  }

  return cached;
};

const deleteExistingPipelines = async () => {
  const supabase = getSupabaseClient();
  const existing = await supabase.from("crm_deal_pipelines").select("id");

  if (existing.error) {
    return existing.error;
  }

  const existingIds = (existing.data ?? []).map((pipeline) => pipeline.id);
  if (existingIds.length > 0) {
    const stagesDeleteResult = await supabase
      .from("crm_deal_pipeline_stages")
      .delete()
      .in("pipeline_id", existingIds);
    if (stagesDeleteResult.error) {
      return stagesDeleteResult.error;
    }
    const pipelinesDeleteResult = await supabase
      .from("crm_deal_pipelines")
      .delete()
      .in("id", existingIds);

    return pipelinesDeleteResult.error ?? null;
  }

  return null;
};

const saveLegacyDealStages = async (pipelines: DealPipeline[]) => {
  const supabase = getSupabaseClient();
  const legacyPipeline = pipelines[0];
  const needsPipelineTables =
    pipelines.length > 1
      ? new Error(
          "Multiple pipelines require crm_deal_pipelines tables. Apply crm_customization_schema_v1.sql to enable this.",
        )
      : null;

  if (!legacyPipeline) {
    return { error: needsPipelineTables } as const;
  }

  const existingStages = await supabase
    .from("crm_deal_stages")
    .select("key");
  if (existingStages.error) {
    return { error: existingStages.error } as const;
  }

  const existingKeys = (existingStages.data ?? []).map((stage) => stage.key);
  if (existingKeys.length > 0) {
    const deleteResult = await supabase
      .from("crm_deal_stages")
      .delete()
      .in("key", existingKeys);
    if (deleteResult.error) {
      return { error: deleteResult.error } as const;
    }
  }

  const legacyStagePayload = legacyPipeline.stages.map((stage) => ({
    key: stage.key,
    label: stage.label,
    order: stage.order,
    is_closed_won: stage.isClosedWon,
    is_closed_lost: stage.isClosedLost,
  })) satisfies Database["public"]["Tables"]["crm_deal_stages"]["Insert"][];

  if (legacyStagePayload.length > 0) {
    const insertResult = await supabase
      .from("crm_deal_stages")
      .insert(legacyStagePayload);
    if (insertResult.error) {
      return { error: insertResult.error } as const;
    }
  }

  return { error: needsPipelineTables } as const;
};

export const saveDealPipelines = async (pipelines: DealPipeline[]) => {
  saveStoredDealPipelines(pipelines);

  try {
    const deleteError = await deleteExistingPipelines();
    if (deleteError) {
      console.error("Failed to clear CRM pipelines", deleteError);
      if (isMissingPipelineTables(deleteError)) {
        return saveLegacyDealStages(pipelines);
      }
      return { error: deleteError } as const;
    }

    const supabase = getSupabaseClient();
    const pipelinePayload = pipelines.map((pipeline) => ({
      id: pipeline.id,
      name: pipeline.name,
      description: pipeline.description ?? "",
    })) satisfies Database["public"]["Tables"]["crm_deal_pipelines"]["Insert"][];

    const pipelinesInsertResult = await supabase
      .from("crm_deal_pipelines")
      .insert(pipelinePayload);

    if (pipelinesInsertResult.error) {
      console.error(
        "Failed to persist CRM pipelines",
        pipelinesInsertResult.error,
      );
      if (isMissingPipelineTables(pipelinesInsertResult.error)) {
        return saveLegacyDealStages(pipelines);
      }
      return { error: pipelinesInsertResult.error } as const;
    }

    const stagePayload = pipelines.flatMap((pipeline) =>
      pipeline.stages.map((stage) => ({
        pipeline_id: pipeline.id,
        stage_key: stage.key,
        label: stage.label,
        order: stage.order,
        is_closed_won: stage.isClosedWon,
        is_closed_lost: stage.isClosedLost,
        is_active: stage.isActive,
      })),
    ) satisfies Database["public"]["Tables"]["crm_deal_pipeline_stages"]["Insert"][];

    if (stagePayload.length > 0) {
      const stagesInsertResult = await supabase
        .from("crm_deal_pipeline_stages")
        .insert(stagePayload);
      if (stagesInsertResult.error) {
        console.error(
          "Failed to persist CRM pipeline stages",
          stagesInsertResult.error,
        );
        if (isMissingPipelineTables(stagesInsertResult.error)) {
          return saveLegacyDealStages(pipelines);
        }
        return { error: stagesInsertResult.error } as const;
      }
    }
  } catch (error) {
    console.error("Failed to persist CRM pipelines", error);
    return { error: error as Error } as const;
  }

  return { error: null } as const;
};

export const loadStoredPipelineSelection = () =>
  loadStoredValue<string>(selectedPipelineStorageKey);

export const saveStoredPipelineSelection = (pipelineId: string) => {
  saveStoredValue(selectedPipelineStorageKey, pipelineId);
};

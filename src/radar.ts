export interface RadarModel {
  id: string;
  model: string;
  modelLabel: string;
  effort: string;
  score?: number | null;
  scoreMax: number;
  status: string;
  itemsScored: number;
  itemsExpected: number;
  wallTime?: string | null;
  costUsd?: number | null;
  costKind: string;
  costKnown: number;
  costTotal: number;
  communityScore?: number | null;
  communityVotes?: number | null;
}

export interface RadarSnapshot {
  models: RadarModel[];
  scoreMax?: number | null;
  status?: string | null;
  batch?: string | null;
  updatedAt?: string | null;
  fetchedAt?: number | null;
  source: string;
  attribution: string;
  siteUrl: string;
  cached: boolean;
  error?: string | null;
}

export interface RadarGroup {
  id: string;
  label: string;
  efforts: RadarModel[];
}

const EFFORT_ORDER = ["ultra", "max", "xhigh", "high", "medium", "low"];

function effortRank(effort: string) {
  const index = EFFORT_ORDER.indexOf(effort.toLowerCase());
  return index < 0 ? EFFORT_ORDER.length : index;
}

export function groupRadarModels(models: readonly RadarModel[]): RadarGroup[] {
  const groups = new Map<string, RadarGroup>();
  const seen = new Set<string>();
  for (const model of models) {
    const id = model.model.toLowerCase();
    const key = `${id}|${model.effort.toLowerCase()}`;
    if (seen.has(key)) continue;
    seen.add(key);
    let group = groups.get(id);
    if (!group) {
      group = { id, label: model.modelLabel, efforts: [] };
      groups.set(id, group);
    }
    group.efforts.push(model);
  }
  // Map insertion order preserves the website order; never rank model families
  // alphabetically, by score, or by an outdated hard-coded model list.
  return [...groups.values()].map((group) => ({
    ...group,
    efforts: group.efforts.sort((a, b) => effortRank(a.effort) - effortRank(b.effort)),
  }));
}

export function radarScoreLabel(model: RadarModel) {
  if (typeof model.score === "number" && Number.isFinite(model.score)) return "综合分";
  if (model.status === "unavailable") return "暂不可用";
  if (model.status === "pending") return "待核验";
  return model.itemsScored > 0 ? "评分未齐" : "待评分";
}

export function radarCostTitle(model: RadarModel) {
  const kind = { actual: "实际费用", estimated: "估算费用", mixed: "实际与估算费用", unknown: "费用暂缺" }[model.costKind] ?? "参考费用";
  return `${kind} · 已知 ${model.costKnown}/${model.costTotal} 项，含动画生成、不含自动评分费用${model.costKnown < model.costTotal ? "；当前仅为已知费用小计" : ""}`;
}

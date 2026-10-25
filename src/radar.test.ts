import { describe, expect, it } from "vitest";
import { groupRadarModels, radarScoreLabel, type RadarModel } from "./radar";

function model(family: string, effort: string): RadarModel {
  return { id: `${family}-${effort}`, model: family, modelLabel: family, effort, score: 120,
    scoreMax: 150, status: "scored", itemsScored: 10, itemsExpected: 10,
    costKind: "unknown", costKnown: 0, costTotal: 10 };
}

describe("grouped Radar layout", () => {
  it("retains the feed's model order even when families are interleaved", () => {
    const groups = groupRadarModels([
      model("gpt-6-astra", "low"), model("gpt-6.1-sol", "max"),
      model("gpt-6-astra", "ultra"), model("a-future-model", "high"),
    ]);
    expect(groups.map((group) => group.id)).toEqual(["gpt-6-astra", "gpt-6.1-sol", "a-future-model"]);
  });

  it("sorts all six efforts from highest to lowest without mutating input", () => {
    const input = ["low", "medium", "high", "xhigh", "max", "ultra"].map((effort) => model("gpt-6-astra", effort));
    expect(groupRadarModels(input)[0].efforts.map((item) => item.effort)).toEqual(["ultra", "max", "xhigh", "high", "medium", "low"]);
    expect(input[0].effort).toBe("low");
  });

  it("starts with max when ultra is absent and keeps future additional efforts", () => {
    const groups = groupRadarModels(["low", "new-effort", "high", "max", "medium", "xhigh", "future-effort"].map((effort) => model("gpt-6-luna", effort)));
    expect(groups[0].efforts.map((item) => item.effort)).toEqual(["max", "xhigh", "high", "medium", "low", "new-effort", "future-effort"]);
    expect(Math.ceil(groups[0].efforts.length / 3)).toBe(3);
  });

  it("does not duplicate a model/effort cell or turn unscored cells into zero", () => {
    const scored = model("gpt-6-astra", "ultra");
    expect(groupRadarModels([scored, { ...scored, effort: "ULTRA" }])[0].efforts).toHaveLength(1);
    expect(radarScoreLabel({ ...scored, score: null, status: "incomplete", itemsScored: 9 })).toBe("评分未齐");
    expect(radarScoreLabel({ ...scored, score: 0 })).toBe("综合分");
  });
});

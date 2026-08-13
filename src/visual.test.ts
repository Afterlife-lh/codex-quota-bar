import { describe, expect, it } from "vitest";
import { formatCountdown, formatRadarCost, formatRadarScore, quotaColor, radarColor, ringGeometry, ringProgress } from "./visual";

describe("quotaColor", () => {
  it("uses the exact dark anchors", () => {
    expect(quotaColor(0, true)).toBe("#ff5a5f");
    expect(quotaColor(50, true)).toBe("#f6c344");
    expect(quotaColor(100, true)).toBe("#43d17a");
  });

  it("clamps values", () => {
    expect(quotaColor(-5, false)).toBe("#c62828");
    expect(quotaColor(105, false)).toBe("#147a3f");
  });
});

describe("ringProgress", () => {
  it("handles exact empty and full rings without SVG dash seams", () => {
    expect(ringProgress(0)).toEqual({ value: 0, empty: true, full: false, dashOffset: 100 });
    expect(ringProgress(100)).toEqual({ value: 100, empty: false, full: true, dashOffset: 0 });
  });

  it("clamps values and builds a normalized partial arc", () => {
    expect(ringProgress(64).dashOffset).toBe(36);
    expect(ringProgress(99.6).full).toBe(true);
    expect(ringProgress(0.4).empty).toBe(true);
    expect(ringProgress(-1).empty).toBe(true);
    expect(ringProgress(101).full).toBe(true);
  });

  it("maps 74 percent to exactly 74 percent of the real circumference", () => {
    const geometry = ringGeometry(74, 11.7);
    expect(geometry.visibleLength / geometry.circumference).toBeCloseTo(.74, 12);
    expect(geometry.dashOffset / geometry.circumference).toBeCloseTo(.26, 12);
  });
});

describe("formatCountdown", () => {
  it("formats short and long windows", () => {
    const now = 1_000_000;
    expect(formatCountdown(now + 3_661_000, true, now)).toBe("1h1m");
    expect(formatCountdown(now + 2 * 86_400_000 + 4 * 3_600_000, true, now)).toBe("2d4h");
  });
});

describe("Radar metric formatting", () => {
  it("renders upstream null and invalid numeric values as unavailable", () => {
    expect(formatRadarScore(null)).toBe("--");
    expect(formatRadarScore(Number.NaN)).toBe("--");
    expect(formatRadarCost(null)).toBe("--");
    expect(formatRadarCost(Number.POSITIVE_INFINITY)).toBe("--");
    expect(radarColor(null)).toBe("#8c96a8");
  });

  it("keeps valid scores and costs formatted consistently", () => {
    expect(formatRadarScore(101.45)).toBe("101.5");
    expect(formatRadarCost(9.406)).toBe("$9.4");
    expect(formatRadarCost(105.2)).toBe("$105");
  });
});

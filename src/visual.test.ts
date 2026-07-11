import { describe, expect, it } from "vitest";
import { formatCountdown, quotaColor } from "./visual";

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

describe("formatCountdown", () => {
  it("formats short and long windows", () => {
    const now = 1_000_000;
    expect(formatCountdown(now + 3_661_000, true, now)).toBe("1h1m");
    expect(formatCountdown(now + 2 * 86_400_000 + 4 * 3_600_000, true, now)).toBe("2d4h");
  });
});

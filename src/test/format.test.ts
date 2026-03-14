import { describe, it, expect } from "vitest";
import { usToMs, usToSec, formatFrameRate } from "../lib/format";

describe("usToMs", () => {
  it("converts microseconds to milliseconds", () => {
    expect(usToMs(33333)).toBe("33.33");
  });
  it("returns dash for null", () => {
    expect(usToMs(null)).toBe("—");
  });
  it("handles zero", () => {
    expect(usToMs(0)).toBe("0.00");
  });
});

describe("usToSec", () => {
  it("converts microseconds to seconds", () => {
    expect(usToSec(1500000)).toBe("1.500");
  });
  it("returns dash for null", () => {
    expect(usToSec(null)).toBe("—");
  });
});

describe("formatFrameRate", () => {
  it("computes fps from rational", () => {
    expect(formatFrameRate(30000, 1001)).toBe("29.97");
  });
  it("returns dash for zero denominator", () => {
    expect(formatFrameRate(30, 0)).toBe("—");
  });
});

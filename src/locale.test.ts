import { describe, expect, it } from "vitest";
import { normalizeLocale } from "./locale";

describe("normalizeLocale", () => {
  it("keeps supported locales", () => {
    expect(normalizeLocale("zh-CN")).toBe("zh-CN");
    expect(normalizeLocale("en")).toBe("en");
  });
  it("falls back for untrusted persisted values", () => {
    expect(normalizeLocale("fr")).toBe("zh-CN");
    expect(normalizeLocale(null)).toBe("zh-CN");
  });
});

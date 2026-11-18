export const supportedLocales = ["zh-CN", "en"] as const;

export type AppLocale = (typeof supportedLocales)[number];

export function normalizeLocale(value: unknown): AppLocale {
  return value === "en" ? "en" : "zh-CN";
}

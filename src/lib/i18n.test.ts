import { afterEach, describe, expect, it, vi } from "vitest";
import { i18n, setLocale, systemLocale, t, toggleLocale } from "./i18n.svelte";

const originalLocale = i18n.locale;
afterEach(() => { i18n.locale = originalLocale; vi.unstubAllGlobals(); });

describe("locale selection", () => {
  it("chooses the first supported OS language including regional and case variants", () => {
    expect(systemLocale(["de-DE", "FR-ca", "en-US"])).toBe("fr");
    expect(systemLocale(["en-GB", "fr-FR"])).toBe("en");
    expect(systemLocale(["ja-JP"])).toBe("en");
    expect(systemLocale([])).toBe("en");
  });

  it("changes document language and persists an explicit choice across toggles", () => {
    const values = new Map<string, string>();
    const root = { lang: "" };
    vi.stubGlobal("document", { documentElement: root });
    vi.stubGlobal("localStorage", { setItem: (key: string, value: string) => values.set(key, value) });
    setLocale("fr");
    expect(i18n.locale).toBe("fr");
    expect(root.lang).toBe("fr");
    expect(values.get("herbarium.locale")).toBe("fr");
    toggleLocale();
    expect(i18n.locale).toBe("en");
    expect(root.lang).toBe("en");
    expect(values.get("herbarium.locale")).toBe("en");
  });

  it("retains a usable locale if preference storage refuses writes", () => {
    vi.stubGlobal("document", { documentElement: { lang: "" } });
    vi.stubGlobal("localStorage", { setItem: () => { throw new Error("storage denied"); } });
    setLocale("fr");
    expect(i18n.locale).toBe("fr");
    const message = t("edit.shortcut", { key: "⌘S" });
    expect(message).toContain("⌘S");
    expect(message).not.toContain("{key}");
  });
});

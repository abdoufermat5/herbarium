import { afterEach, describe, expect, it, vi } from "vitest";
import { navigate, registerLeaveGuard } from "./navigation.svelte";

const unregister: Array<() => void> = [];
afterEach(() => { unregister.splice(0).forEach((stop) => stop()); vi.restoreAllMocks(); });

describe("unsaved-work navigation", () => {
  it("leaves the reader untouched on veto and allows navigation after unregistering", async () => {
    let route = "reader";
    const stop = registerLeaveGuard(async () => false);
    unregister.push(stop);
    expect(await navigate(() => { route = "library"; })).toBe(false);
    expect(route).toBe("reader");
    stop();
    expect(await navigate(() => { route = "library"; })).toBe(true);
    expect(route).toBe("library");
  });

  it("serializes competing leave requests against the state left by the first action", async () => {
    let release!: (value: boolean) => void;
    const answer = new Promise<boolean>((resolve) => { release = resolve; });
    let dirty = true;
    let prompts = 0;
    const order: string[] = [];
    unregister.push(registerLeaveGuard(async () => {
      if (!dirty) return true;
      prompts++;
      return answer;
    }));
    const first = navigate(() => { dirty = false; order.push("library"); });
    const second = navigate(() => { order.push("settings"); });
    await Promise.resolve();
    expect(prompts).toBe(1);
    expect(order).toEqual([]);
    release(true);
    expect(await Promise.all([first, second])).toEqual([true, true]);
    expect(prompts).toBe(1);
    expect(order).toEqual(["library", "settings"]);
  });

  it("vetoes a failed guard and recovers its queue after an action rejects", async () => {
    vi.spyOn(console, "error").mockImplementation(() => {});
    let route = "reader";
    const stop = registerLeaveGuard(async () => { throw new Error("prompt unavailable"); });
    unregister.push(stop);
    expect(await navigate(() => { route = "library"; })).toBe(false);
    expect(route).toBe("reader");
    stop();
    await expect(navigate(async () => { throw new Error("vault unavailable"); })).rejects.toThrow("vault unavailable");
    expect(await navigate(() => { route = "settings"; })).toBe(true);
    expect(route).toBe("settings");
  });
});

// Guarded navigation. Views with unsaved work (the reader's editor and
// inspector) register a leave guard; every route change goes through
// `navigate`, which asks each guard in turn and only runs the change when all
// agree. Requests are serialized, so two quick clicks never stack two prompts
// and the second is decided against the state the first left behind.

/** Resolves true when it is fine to leave (nothing dirty, or the user chose to discard/save). */
export type LeaveGuard = () => Promise<boolean>;

const guards = new Set<LeaveGuard>();
let queue: Promise<unknown> = Promise.resolve();

/** Register a guard; returns the function that unregisters it (call it on destroy). */
export function registerLeaveGuard(fn: LeaveGuard): () => void {
  // A fresh wrapper so registering the same function twice yields two independent entries.
  const guard: LeaveGuard = () => fn();
  guards.add(guard);
  return () => {
    guards.delete(guard);
  };
}

/** True while any view could veto leaving. */
export function hasLeaveGuards(): boolean {
  return guards.size > 0;
}

/** Ask every guard, without navigating. A guard that throws vetoes. */
export async function canLeave(): Promise<boolean> {
  for (const guard of [...guards]) {
    try {
      if (!(await guard())) return false;
    } catch (e) {
      console.error(e);
      return false;
    }
  }
  return true;
}

/**
 * Run `action` once every guard allows leaving. Resolves false when a guard
 * vetoed (the action did not run); rejects when `action` throws. Guards must
 * not call `navigate` themselves (the queue is serial).
 */
export function navigate(action: () => void | Promise<void>): Promise<boolean> {
  const run = queue.then(async () => {
    if (!(await canLeave())) return false;
    await action();
    return true;
  });
  queue = run.catch(() => undefined);
  return run;
}

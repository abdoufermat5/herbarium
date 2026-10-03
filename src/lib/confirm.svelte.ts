// In-app confirmation dialog: `confirmAction()` opens it (rendered once by App)
// and resolves to the user's answer.

import type { IconName } from "./icons";

export interface ConfirmOptions {
  title: string;
  message: string;
  confirmLabel: string;
  /** Styles the confirm button as destructive. */
  danger?: boolean;
  /** The thing being acted on, shown as a card between the title and the message. */
  subject?: { icon: IconName; label: string; meta?: string };
}

interface Pending extends ConfirmOptions {
  resolve: (ok: boolean) => void;
}

export const confirmState = $state<{ pending: Pending | null }>({ pending: null });

export function confirmAction(options: ConfirmOptions): Promise<boolean> {
  // A newer request supersedes one still open: that one counts as cancelled.
  confirmState.pending?.resolve(false);
  return new Promise((resolve) => {
    confirmState.pending = { ...options, resolve };
  });
}

export function settleConfirm(ok: boolean) {
  const pending = confirmState.pending;
  if (!pending) return;
  confirmState.pending = null;
  pending.resolve(ok);
}

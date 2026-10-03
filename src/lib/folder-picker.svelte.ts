// Folder chooser dialog: `pickFolder()` opens it (rendered once by App) and
// resolves to the choice. `null` is the vault root, `undefined` means the user
// cancelled.

export interface PickFolderOptions {
  title?: string;
  /** Folder preselected when the dialog opens; null is the root. */
  initial?: string | null;
  /** This folder and everything under it is shown but cannot be chosen (moving a folder into itself). */
  exclude?: string;
}

interface Pending extends PickFolderOptions {
  resolve: (folder: string | null | undefined) => void;
}

export const folderPickerState = $state<{ pending: Pending | null }>({ pending: null });

export function pickFolder(options: PickFolderOptions = {}): Promise<string | null | undefined> {
  // A newer request supersedes one still open: that one counts as cancelled.
  folderPickerState.pending?.resolve(undefined);
  return new Promise((resolve) => {
    folderPickerState.pending = { ...options, resolve };
  });
}

export function settleFolder(folder: string | null | undefined) {
  const pending = folderPickerState.pending;
  if (!pending) return;
  folderPickerState.pending = null;
  pending.resolve(folder);
}

/** True when `path` is `exclude` or lies beneath it. */
export function isExcluded(path: string, exclude: string | undefined): boolean {
  return !!exclude && (path === exclude || path.startsWith(`${exclude}/`));
}

// Exports that leave the app: one page as a self-contained HTML file, or the
// vault (or a folder) as a static website. Both ask where to write first.
import { open, save } from "@tauri-apps/plugin-dialog";
import { api } from "./api";
import { vaultName } from "./format";
import { t } from "./i18n.svelte";
import { app, errorMessage, toast } from "./state.svelte";
import type { PageMeta } from "./types";

/** A file name from a title: letters, digits and dashes. */
function fileStem(title: string, fallback: string): string {
  const stem = title
    .normalize("NFKD")
    .replace(/[^\w\s-]/g, "")
    .trim()
    .replace(/\s+/g, "-")
    .slice(0, 80);
  return stem || fallback;
}

/** Save one page as a standalone HTML file; resolves true once written. */
export async function exportPageFile(page: Pick<PageMeta, "id" | "title">): Promise<boolean> {
  try {
    const dest = await save({
      defaultPath: `${fileStem(page.title, page.id)}.html`,
      title: t("export.pageDialog"),
      filters: [{ name: "HTML", extensions: ["html"] }],
    });
    if (typeof dest !== "string") return false;
    await api.exportPageHtml(page.id, dest);
    toast(t("export.pageDone", { path: dest }), "success");
    return true;
  } catch (e) {
    toast(`${t("export.failed")}: ${errorMessage(e)}`, "error");
    return false;
  }
}

/** Publish the vault, or `folder` and its subfolders, as a static website. */
export async function publishSite(folder: string | null): Promise<boolean> {
  try {
    const dest = await open({ directory: true, title: t("export.siteDialog") });
    if (typeof dest !== "string") return false;
    const title = folder ? folder.split("/").pop()! : vaultName(app.config?.vaultPath ?? "") || "Herbarium";
    const report = await api.exportSite(folder, title, dest);
    toast(t("export.siteDone", { count: report.pages, path: report.index }), "success");
    return true;
  } catch (e) {
    toast(`${t("export.failed")}: ${errorMessage(e)}`, "error");
    return false;
  }
}

// SVG icon paths from lucide (https://lucide.dev) — ISC License.
// Bundled statically so the app never needs network access for chrome UI.

export type IconName =
  | "leaf"
  | "files"
  | "refresh-cw"
  | "folder"
  | "hash"
  | "search"
  | "plus"
  | "upload"
  | "x"
  | "chevron-right"
  | "chevron-down"
  | "layout-grid"
  | "rows-3"
  | "info"
  | "trash-2"
  | "arrow-left"
  | "check"
  | "calendar-clock"
  | "command"
  | "arrow-up-down"
  | "clock"
  | "file-text"
  | "wifi"
  | "wifi-off"
  | "circle-check"
  | "folder-open"
  | "file-plus"
  | "corner-down-left"
  | "sun"
  | "moon"
  | "settings"
  | "languages";

export const icons: Record<IconName, string> = {
  "leaf":
    `<path d="M11 20A7 7 0 0 1 9.8 6.1C15.5 5 17 4.48 19 2c1 2 2 4.18 2 8 0 5.5-4.78 10-10 10Z" /> <path d="M2 21c0-3 1.85-5.36 5.08-6C9.5 14.52 12 13 13 12" />`,
  "files":
    `<path d="M20 7h-3a2 2 0 0 1-2-2V2" /> <path d="M9 18a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h7l4 4v10a2 2 0 0 1-2 2Z" /> <path d="M3 7.6v12.8A1.6 1.6 0 0 0 4.6 22h9.8" />`,
  "refresh-cw":
    `<path d="M3 12a9 9 0 0 1 9-9 9.75 9.75 0 0 1 6.74 2.74L21 8" /> <path d="M21 3v5h-5" /> <path d="M21 12a9 9 0 0 1-9 9 9.75 9.75 0 0 1-6.74-2.74L3 16" /> <path d="M8 16H3v5" />`,
  "folder":
    `<path d="M20 20a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.69-.9L9.6 3.9A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2Z" />`,
  "hash":
    `<line x1="4" x2="20" y1="9" y2="9" /> <line x1="4" x2="20" y1="15" y2="15" /> <line x1="10" x2="8" y1="3" y2="21" /> <line x1="16" x2="14" y1="3" y2="21" />`,
  "search":
    `<circle cx="11" cy="11" r="8" /> <path d="m21 21-4.3-4.3" />`,
  "plus":
    `<path d="M5 12h14" /> <path d="M12 5v14" />`,
  "upload":
    `<path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4" /> <polyline points="17 8 12 3 7 8" /> <line x1="12" x2="12" y1="3" y2="15" />`,
  "x":
    `<path d="M18 6 6 18" /> <path d="m6 6 12 12" />`,
  "chevron-right":
    `<path d="m9 18 6-6-6-6" />`,
  "chevron-down":
    `<path d="m6 9 6 6 6-6" />`,
  "layout-grid":
    `<rect width="7" height="7" x="3" y="3" rx="1" /> <rect width="7" height="7" x="14" y="3" rx="1" /> <rect width="7" height="7" x="14" y="14" rx="1" /> <rect width="7" height="7" x="3" y="14" rx="1" />`,
  "rows-3":
    `<rect width="18" height="18" x="3" y="3" rx="2" /> <path d="M21 9H3" /> <path d="M21 15H3" />`,
  "info":
    `<circle cx="12" cy="12" r="10" /> <path d="M12 16v-4" /> <path d="M12 8h.01" />`,
  "trash-2":
    `<path d="M3 6h18" /> <path d="M19 6v14c0 1-1 2-2 2H7c-1 0-2-1-2-2V6" /> <path d="M8 6V4c0-1 1-2 2-2h4c1 0 2 1 2 2v2" /> <line x1="10" x2="10" y1="11" y2="17" /> <line x1="14" x2="14" y1="11" y2="17" />`,
  "arrow-left":
    `<path d="m12 19-7-7 7-7" /> <path d="M19 12H5" />`,
  "check":
    `<path d="M20 6 9 17l-5-5" />`,
  "calendar-clock":
    `<path d="M21 7.5V6a2 2 0 0 0-2-2H5a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h3.5" /> <path d="M16 2v4" /> <path d="M8 2v4" /> <path d="M3 10h5" /> <path d="M17.5 17.5 16 16.3V14" /> <circle cx="16" cy="16" r="6" />`,
  "command":
    `<path d="M15 6v12a3 3 0 1 0 3-3H6a3 3 0 1 0 3 3V6a3 3 0 1 0-3 3h12a3 3 0 1 0-3-3" />`,
  "arrow-up-down":
    `<path d="m21 16-4 4-4-4" /> <path d="M17 20V4" /> <path d="m3 8 4-4 4 4" /> <path d="M7 4v16" />`,
  "clock":
    `<circle cx="12" cy="12" r="10" /> <polyline points="12 6 12 12 16 14" />`,
  "file-text":
    `<path d="M15 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7Z" /> <path d="M14 2v4a2 2 0 0 0 2 2h4" /> <path d="M10 9H8" /> <path d="M16 13H8" /> <path d="M16 17H8" />`,
  "wifi":
    `<path d="M12 20h.01" /> <path d="M2 8.82a15 15 0 0 1 20 0" /> <path d="M5 12.859a10 10 0 0 1 14 0" /> <path d="M8.5 16.429a5 5 0 0 1 7 0" />`,
  "wifi-off":
    `<path d="M12 20h.01" /> <path d="M8.5 16.429a5 5 0 0 1 7 0" /> <path d="M5 12.859a10 10 0 0 1 5.17-2.69" /> <path d="M19 12.859a10 10 0 0 0-2.007-1.523" /> <path d="M2 8.82a15 15 0 0 1 4.177-2.643" /> <path d="M22 8.82a15 15 0 0 0-11.288-3.764" /> <path d="m2 2 20 20" />`,
  "circle-check":
    `<circle cx="12" cy="12" r="10" /> <path d="m9 12 2 2 4-4" />`,
  "folder-open":
    `<path d="m6 14 1.5-2.9A2 2 0 0 1 9.24 10H20a2 2 0 0 1 1.94 2.5l-1.54 6a2 2 0 0 1-1.95 1.5H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h3.9a2 2 0 0 1 1.69.9l.81 1.2a2 2 0 0 0 1.67.9H18a2 2 0 0 1 2 2v2" />`,
  "file-plus":
    `<path d="M15 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7Z" /> <path d="M14 2v4a2 2 0 0 0 2 2h4" /> <path d="M9 15h6" /> <path d="M12 18v-6" />`,
  "corner-down-left":
    `<polyline points="9 10 4 15 9 20" /> <path d="M20 4v7a4 4 0 0 1-4 4H4" />`,
  "sun":
    `<circle cx="12" cy="12" r="4" /> <path d="M12 2v2" /> <path d="M12 20v2" /> <path d="m4.93 4.93 1.41 1.41" /> <path d="m17.66 17.66 1.41 1.41" /> <path d="M2 12h2" /> <path d="M20 12h2" /> <path d="m6.34 17.66-1.41 1.41" /> <path d="m19.07 4.93-1.41 1.41" />`,
  "languages":
    `<path d="m5 8 6 6" /> <path d="m4 14 6-6 2-3" /> <path d="M2 5h12" /> <path d="M7 2h1" /> <path d="m22 22-5-10-5 10" /> <path d="M14 18h6" />`,
  "settings":
    `<path d="M12 8v8" /> <path d="M4 4v16" /> <path d="M20 4v16" /> <path d="M12 4v1" /> <path d="M12 19v1" /> <path d="M1 8h6" /> <path d="M9 16h6" /> <path d="M17 8h6" />`,
  "moon":
    `<path d="M12 3a6 6 0 0 0 9 9 9 9 0 1 1-9-9Z" />`,
};

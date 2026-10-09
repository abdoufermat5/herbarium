// Chrome only: watches the system's light or dark mode, which the service
// worker cannot, and tells it so the toolbar icon matches.
"use strict";

const ext = globalThis.browser ?? globalThis.chrome;
const dark = matchMedia("(prefers-color-scheme: dark)");
const report = () => ext.runtime.sendMessage({ type: "colorScheme", dark: dark.matches }).catch(() => {});
dark.addEventListener("change", report);
report();

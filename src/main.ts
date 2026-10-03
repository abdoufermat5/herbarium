import { mount } from "svelte";
import "@fontsource-variable/geist";
import "@fontsource-variable/geist-mono";
import "@fontsource-variable/newsreader/opsz.css";
import "./app.css";
import App from "./App.svelte";
import { initApp, initTheme } from "./lib/state.svelte";
import { initLocale } from "./lib/i18n.svelte";
import { initLinkPrompts } from "./lib/links";

initTheme();
initLocale();
initLinkPrompts();
initApp().catch((err) => console.error("Init failed:", err));

const app = mount(App, { target: document.getElementById("app")! });

export default app;
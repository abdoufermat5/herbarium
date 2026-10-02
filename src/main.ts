import { mount } from "svelte";
import "./app.css";
import App from "./App.svelte";
import { initApp, initTheme } from "./lib/state.svelte";
import { initLocale } from "./lib/i18n.svelte";

initTheme();
initLocale();
initApp().catch((err) => console.error("Init failed:", err));

const app = mount(App, { target: document.getElementById("app")! });

export default app;
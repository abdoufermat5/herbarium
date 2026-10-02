import { mount } from "svelte";
import "./app.css";
import App from "./App.svelte";
import { initApp } from "./lib/state.svelte";

initApp().catch((err) => console.error("Init failed:", err));

const app = mount(App, { target: document.getElementById("app")! });

export default app;
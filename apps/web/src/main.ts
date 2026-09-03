import { mount } from "svelte";
import "@notedock/editor/styles/tokens.css";
import "@notedock/editor/styles/base.css";
import "@notedock/editor/styles/prose.css";
import "./app.css";
import App from "./App.svelte";

const target = document.getElementById("app");
if (!target) throw new Error("missing #app mount point");

export default mount(App, { target });

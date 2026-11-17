import { mount } from "svelte";
import "./app.css";
import App from "./App.svelte";
import Notch from "./lib/Notch.svelte";

const target = document.getElementById("app");
// The notch panel is a second window that loads the same bundle with `#notch`.
if (target) mount(location.hash === "#notch" ? Notch : App, { target });

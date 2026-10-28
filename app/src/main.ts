import { createApp } from "vue";
import App from "./App.vue";
import { applyTheme, cachedTheme } from "./appearance";
import "./style.css";

applyTheme(cachedTheme());
createApp(App).mount("#app");

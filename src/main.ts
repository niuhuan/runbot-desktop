import { createApp } from "vue";
import App from "./App.vue";

// 非开发模式下禁用右键菜单
if (!import.meta.env.DEV) {
  document.addEventListener('contextmenu', (e) => {
    e.preventDefault();
  });
}

createApp(App).mount("#app");

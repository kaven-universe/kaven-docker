import { createApp } from "vue";
import { createPinia } from "pinia";
import { QueryClient, VueQueryPlugin } from "@tanstack/vue-query";
import App from "./App.vue";
import { i18n } from "./i18n";
import "./styles.css";

const app = createApp(App);
app.use(createPinia());
app.use(i18n);
app.use(VueQueryPlugin, {
  queryClient: new QueryClient({
    defaultOptions: {
      queries: { retry: 1, staleTime: 20_000, refetchOnWindowFocus: false },
    },
  }),
});
app.mount("#app");

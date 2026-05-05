import { createRouter, createWebHistory } from "vue-router";

import DocumentPage from "../pages/DocumentPage.vue";
import DocumentsPage from "../pages/DocumentsPage.vue";
import LoginPage from "../pages/LoginPage.vue";

export const router = createRouter({
  history: createWebHistory(),
  routes: [
    { path: "/login", component: LoginPage },
    { path: "/", component: DocumentsPage },
    { path: "/documents/:id", component: DocumentPage },
  ],
});

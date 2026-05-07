import { createRouter, createWebHistory } from "vue-router";

import AdminUsersPage from "../pages/AdminUsersPage.vue";
import DocumentComparePage from "../pages/DocumentComparePage.vue";
import DocumentOutlinePage from "../pages/DocumentOutlinePage.vue";
import DocumentPage from "../pages/DocumentPage.vue";
import DocumentsPage from "../pages/DocumentsPage.vue";
import LoginPage from "../pages/LoginPage.vue";
import ProfilePage from "../pages/ProfilePage.vue";
import SectionEditPage from "../pages/SectionEditPage.vue";

export const router = createRouter({
  history: createWebHistory(),
  routes: [
    { path: "/login", component: LoginPage },
    { path: "/profile", component: ProfilePage },
    { path: "/admin/users", component: AdminUsersPage },
    { path: "/", component: DocumentsPage },
    { path: "/documents/:id", component: DocumentPage },
    { path: "/documents/:id/compare", component: DocumentComparePage },
    { path: "/documents/:id/outline", component: DocumentOutlinePage },
    { path: "/sections/:id/edit", component: SectionEditPage },
  ],
});

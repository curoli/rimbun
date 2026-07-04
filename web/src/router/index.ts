import { createRouter, createWebHistory } from "vue-router";

import AdminUsersPage from "../pages/AdminUsersPage.vue";
import AdminVariantCollectionsPage from "../pages/AdminVariantCollectionsPage.vue";
import AdminSiteSettingsPage from "../pages/AdminSiteSettingsPage.vue";
import DocumentOutlinePage from "../pages/DocumentOutlinePage.vue";
import DocumentPage from "../pages/DocumentPage.vue";
import DocumentSettingsPage from "../pages/DocumentSettingsPage.vue";
import DocumentsPage from "../pages/DocumentsPage.vue";
import LoginPage from "../pages/LoginPage.vue";
import ProfilePage from "../pages/ProfilePage.vue";
import SectionEditPage from "../pages/SectionEditPage.vue";

export const router = createRouter({
  history: createWebHistory(),
  routes: [
    { path: "/login", component: LoginPage },
    { path: "/profile", component: ProfilePage },
    { path: "/admin/site-settings", component: AdminSiteSettingsPage },
    { path: "/admin/users", component: AdminUsersPage },
    { path: "/admin/variant-collections", component: AdminVariantCollectionsPage },
    { path: "/", component: DocumentsPage },
    { path: "/documents/:documentRef", component: DocumentPage },
    { path: "/documents/:documentRef/compare", redirect: (to) => `/documents/${to.params.documentRef}` },
    { path: "/documents/:documentRef/outline", component: DocumentOutlinePage },
    { path: "/documents/:documentRef/settings", component: DocumentSettingsPage },
    { path: "/sections/:id/edit", component: SectionEditPage },
  ],
});

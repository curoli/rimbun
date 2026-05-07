import { apiRequest } from "./client";
import type { AuthSession, User } from "./types";

export type RegisterPayload = {
  username: string;
  display_name: string;
  email: string;
  password: string;
};

export type LoginPayload = {
  identifier: string;
  password: string;
};

export function register(payload: RegisterPayload) {
  return apiRequest<AuthSession>("/api/auth/register", {
    method: "POST",
    bodyJson: payload,
  });
}

export function login(payload: LoginPayload) {
  return apiRequest<AuthSession>("/api/auth/login", {
    method: "POST",
    bodyJson: payload,
  });
}

export function logout() {
  return apiRequest<{ status: string }>("/api/auth/logout", {
    method: "POST",
  });
}

export function me() {
  return apiRequest<User>("/api/me");
}

export function updateProfile(payload: { display_name: string }) {
  return apiRequest<User>("/api/me", {
    method: "PATCH",
    bodyJson: payload,
  });
}

export function changePassword(payload: { current_password: string; new_password: string }) {
  return apiRequest<{ status: string }>("/api/me/change-password", {
    method: "POST",
    bodyJson: payload,
  });
}

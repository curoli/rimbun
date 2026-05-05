import { apiRequest } from "./client";
import type { User } from "./types";

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
  return apiRequest<User>("/api/auth/register", {
    method: "POST",
    bodyJson: payload,
  });
}

export function login(payload: LoginPayload) {
  return apiRequest<User>("/api/auth/login", {
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

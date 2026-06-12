import { apiRequest } from "./client";
import type { User } from "./types";

export function listUsers() {
  return apiRequest<User[]>("/api/users");
}

export function resetUserPassword(userId: string, payload: { new_password: string }) {
  return apiRequest<{ status: string }>(`/api/users/${userId}/reset-password`, {
    method: "POST",
    bodyJson: payload,
  });
}

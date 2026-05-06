const API_BASE_URL = import.meta.env.VITE_API_BASE_URL ?? "";
const SESSION_HEADER_NAME = "x-rimbun-session";
let activeSessionToken: string | null = null;

export class ApiError extends Error {
  status: number;

  constructor(message: string, status: number) {
    super(message);
    this.name = "ApiError";
    this.status = status;
  }
}

type RequestOptions = RequestInit & {
  bodyJson?: unknown;
};

export function setActiveSessionToken(token: string | null) {
  activeSessionToken = token;
}

export function getActiveSessionToken() {
  return activeSessionToken;
}

export async function apiRequest<T>(path: string, options: RequestOptions = {}): Promise<T> {
  const headers = new Headers(options.headers);
  if (options.bodyJson !== undefined) {
    headers.set("Content-Type", "application/json");
  }
  if (activeSessionToken) {
    headers.set(SESSION_HEADER_NAME, activeSessionToken);
  }

  const response = await fetch(`${API_BASE_URL}${path}`, {
    ...options,
    headers,
    credentials: "include",
    body: options.bodyJson !== undefined ? JSON.stringify(options.bodyJson) : options.body,
  });

  if (!response.ok) {
    let message = `Request failed with status ${response.status}`;
    try {
      const data = (await response.json()) as { error?: string };
      if (data.error) {
        message = data.error;
      }
    } catch {
      // Ignore JSON parsing failure and keep generic message.
    }
    throw new ApiError(message, response.status);
  }

  if (response.status === 204) {
    return undefined as T;
  }

  return (await response.json()) as T;
}

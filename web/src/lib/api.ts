import { RefreshResponse } from "./types";

const API_BASE_URL = process.env.NEXT_PUBLIC_API_URL || "http://localhost:8080/v1";

let currentAccessToken: string | null = null;
let refreshPromise: Promise<string | null> | null = null;

export class ApiError extends Error {
  constructor(message: string, public status?: number) {
    super(message);
    this.name = "ApiError";
  }
}

export function setAccessToken(token: string | null) {
  currentAccessToken = token;
}

export function getAccessToken(): string | null {
  return currentAccessToken;
}

export async function refreshAccessToken(): Promise<string | null> {
  if (refreshPromise) {
    return refreshPromise;
  }

  refreshPromise = (async () => {
    try {
      const res = await fetch(`${API_BASE_URL}/auth/refresh`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        credentials: "include", // Sends and receives httpOnly refresh cookie
      });

      if (!res.ok) {
        currentAccessToken = null;
        let errorMessage = `HTTP error ${res.status}`;
        try {
          const errJson = await res.json();
          if (errJson?.error?.message) {
            errorMessage = errJson.error.message;
          }
        } catch {
          // ignore json parse error
        }
        throw new ApiError(errorMessage, res.status);
      }

      const data: RefreshResponse = await res.json();
      currentAccessToken = data.access_token;
      return data.access_token;
    } catch (err) {
      currentAccessToken = null;
      throw err;
    } finally {
      refreshPromise = null;
    }
  })();

  return refreshPromise;
}

export async function apiClient<T>(
  endpoint: string,
  options: RequestInit = {}
): Promise<T> {
  const url = endpoint.startsWith("http") ? endpoint : `${API_BASE_URL}${endpoint}`;
  const headers = new Headers(options.headers || {});

  if (!headers.has("Content-Type") && !(options.body instanceof FormData)) {
    headers.set("Content-Type", "application/json");
  }

  if (currentAccessToken && !headers.has("Authorization")) {
    headers.set("Authorization", `Bearer ${currentAccessToken}`);
  }

  const fetchOptions: RequestInit = {
    ...options,
    headers,
    credentials: "include", // Ensures httpOnly cookies are attached across requests
  };

  let response = await fetch(url, fetchOptions);

  // If unauthorized and we haven't already retried with a refresh
  if (response.status === 401 && !endpoint.includes("/auth/refresh") && !endpoint.includes("/auth/google")) {
    try {
      const newAccessToken = await refreshAccessToken();
      if (newAccessToken) {
        headers.set("Authorization", `Bearer ${newAccessToken}`);
        response = await fetch(url, {
          ...fetchOptions,
          headers,
        });
      }
    } catch {
      // Refresh failed, proceed to handle error response below
    }
  }

  if (!response.ok) {
    let errorMessage = `HTTP error ${response.status}`;
    try {
      const errJson = await response.json();
      if (errJson?.error?.message) {
        errorMessage = errJson.error.message;
      }
    } catch {
      // ignore json parse error
    }
    throw new ApiError(errorMessage, response.status);
  }

  return response.json();
}

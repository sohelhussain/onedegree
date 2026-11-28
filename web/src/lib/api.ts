import { AuthResponse, MessageResponse, RefreshResponse } from "./types";

const API_BASE_URL = process.env.NEXT_PUBLIC_API_URL || "http://localhost:8080/v1";

let currentAccessToken: string | null = null;
let refreshPromise: Promise<string | null> | null = null;
let unauthorizedHandler: (() => void) | null = null;

export class ApiError extends Error {
  constructor(
    message: string,
    public status?: number,
    public code?: string
  ) {
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

export function setUnauthorizedHandler(handler: (() => void) | null) {
  unauthorizedHandler = handler;
}

async function parseErrorResponse(res: Response): Promise<{ message: string; code?: string }> {
  let message = `HTTP error ${res.status}`;
  let code: string | undefined;
  try {
    const errJson = await res.json();
    if (errJson?.error?.message) {
      message = errJson.error.message;
    }
    if (errJson?.error?.code) {
      code = errJson.error.code;
    }
  } catch {
    // ignore json parse error
  }
  return { message, code };
}

export async function refreshAccessToken(): Promise<string | null> {
  if (refreshPromise) {
    return refreshPromise;
  }

  refreshPromise = (async () => {
    try {
      const res = await fetch(`${API_BASE_URL}/auth/refresh`, {
        method: "POST",
        credentials: "include",
      });

      if (!res.ok) {
        currentAccessToken = null;
        const { message, code } = await parseErrorResponse(res);
        throw new ApiError(message, res.status, code);
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

  if (
    options.body !== undefined &&
    !headers.has("Content-Type") &&
    !(options.body instanceof FormData)
  ) {
    headers.set("Content-Type", "application/json");
  }

  if (currentAccessToken && !headers.has("Authorization")) {
    headers.set("Authorization", `Bearer ${currentAccessToken}`);
  }

  const fetchOptions: RequestInit = {
    ...options,
    headers,
    credentials: "include",
  };

  let response: Response;
  try {
    response = await fetch(url, fetchOptions);
  } catch {
    throw new Error("Unable to reach authentication server. Please check your connection.");
  }

  const isAuthEndpoint = endpoint.includes("/auth/");
  if (response.status === 401 && !isAuthEndpoint) {
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
      unauthorizedHandler?.();
    }
  }

  if (!response.ok) {
    const { message, code } = await parseErrorResponse(response);
    if (response.status === 401 && !isAuthEndpoint) {
      unauthorizedHandler?.();
    }
    throw new ApiError(message, response.status, code);
  }

  return response.json();
}

export function registerAccount(payload: {
  email: string;
  password: string;
  full_name?: string;
}): Promise<MessageResponse> {
  return apiClient<MessageResponse>("/auth/register", {
    method: "POST",
    body: JSON.stringify(payload),
  });
}

export function loginWithEmailApi(payload: {
  email: string;
  password: string;
}): Promise<AuthResponse> {
  return apiClient<AuthResponse>("/auth/login", {
    method: "POST",
    body: JSON.stringify(payload),
  });
}

export function verifyEmailApi(payload: {
  token: string;
  password: string;
}): Promise<MessageResponse> {
  return apiClient<MessageResponse>("/auth/verify-email", {
    method: "POST",
    body: JSON.stringify(payload),
  });
}

export function resendVerificationApi(email: string): Promise<MessageResponse> {
  return apiClient<MessageResponse>("/auth/resend-verification", {
    method: "POST",
    body: JSON.stringify({ email }),
  });
}

export function forgotPasswordApi(email: string): Promise<MessageResponse> {
  return apiClient<MessageResponse>("/auth/forgot-password", {
    method: "POST",
    body: JSON.stringify({ email }),
  });
}

export function resetPasswordApi(payload: {
  token: string;
  new_password: string;
}): Promise<MessageResponse> {
  return apiClient<MessageResponse>("/auth/reset-password", {
    method: "POST",
    body: JSON.stringify(payload),
  });
}

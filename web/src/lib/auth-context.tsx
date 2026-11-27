"use client";

import React, {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useState,
} from "react";
import { useRouter } from "next/navigation";
import { User, AuthResponse } from "./types";
import {
  apiClient,
  setAccessToken,
  refreshAccessToken,
  setUnauthorizedHandler,
  loginWithEmailApi,
  ApiError,
} from "./api";
import { toast } from "sonner";

interface AuthContextType {
  user: User | null;
  accessToken: string | null;
  isLoading: boolean;
  loginWithGoogle: (idToken: string) => Promise<void>;
  loginWithEmail: (email: string, password: string) => Promise<void>;
  logout: () => Promise<void>;
  updateTimezone: (timezone: string) => Promise<void>;
  refreshSession: () => Promise<boolean>;
}

const AuthContext = createContext<AuthContextType | undefined>(undefined);

export function AuthProvider({ children }: { children: React.ReactNode }) {
  const router = useRouter();
  const [user, setUser] = useState<User | null>(null);
  const [accessToken, setToken] = useState<string | null>(null);
  const [isLoading, setIsLoading] = useState<boolean>(true);

  const clearSessionState = useCallback(() => {
    setAccessToken(null);
    setToken(null);
    setUser(null);
  }, []);

  const refreshSession = useCallback(async (): Promise<boolean> => {
    try {
      const token = await refreshAccessToken();
      if (!token) {
        clearSessionState();
        return false;
      }

      setAccessToken(token);
      setToken(token);

      const me = await apiClient<User>("/me");
      setUser(me);
      return true;
    } catch (err: unknown) {
      clearSessionState();

      if (err instanceof ApiError && err.status === 401) {
        return false;
      }

      toast.error("Unable to reach authentication server. Please check your connection.", {
        id: "auth-network-error",
      });
      return false;
    } finally {
      setIsLoading(false);
    }
  }, [clearSessionState]);

  useEffect(() => {
    setUnauthorizedHandler(() => {
      clearSessionState();
      router.replace("/login");
    });
    return () => {
      setUnauthorizedHandler(null);
    };
  }, [clearSessionState, router]);

  useEffect(() => {
    let active = true;

    async function bootstrapSession() {
      try {
        const token = await refreshAccessToken();
        if (!active) return;

        if (!token) {
          clearSessionState();
          return;
        }

        setAccessToken(token);
        setToken(token);

        const me = await apiClient<User>("/me");
        if (!active) return;
        setUser(me);
      } catch (err: unknown) {
        if (!active) return;
        clearSessionState();

        if (!(err instanceof ApiError && err.status === 401)) {
          toast.error("Unable to reach authentication server. Please check your connection.", {
            id: "auth-network-error",
          });
        }
      } finally {
        if (active) {
          setIsLoading(false);
        }
      }
    }

    void bootstrapSession();
    return () => {
      active = false;
    };
  }, [clearSessionState]);

  const loginWithGoogle = useCallback(async (idToken: string) => {
    try {
      setIsLoading(true);
      const res = await apiClient<AuthResponse>("/auth/google", {
        method: "POST",
        body: JSON.stringify({ id_token: idToken }),
      });

      setAccessToken(res.access_token);
      setToken(res.access_token);
      setUser(res.user);
      toast.success("Signed in with Google successfully!");
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : "Authentication failed";
      toast.error(msg);
      throw err;
    } finally {
      setIsLoading(false);
    }
  }, []);

  const loginWithEmail = useCallback(async (email: string, password: string) => {
    try {
      setIsLoading(true);
      const res = await loginWithEmailApi({ email, password });
      setAccessToken(res.access_token);
      setToken(res.access_token);
      setUser(res.user);
      toast.success("Welcome back!");
    } finally {
      setIsLoading(false);
    }
  }, []);

  const logout = useCallback(async () => {
    try {
      await apiClient("/auth/logout", { method: "POST" });
    } catch {
      // continue clearing local session even if endpoint errors
    } finally {
      clearSessionState();
      toast.info("Logged out");
      router.replace("/login");
    }
  }, [clearSessionState, router]);

  const updateTimezone = useCallback(async (timezone: string) => {
    try {
      const updated = await apiClient<User>("/me", {
        method: "PATCH",
        body: JSON.stringify({ timezone }),
      });
      setUser(updated);
      toast.success(`Timezone updated to ${timezone}`);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : "Failed to update timezone";
      toast.error(msg);
      throw err;
    }
  }, []);

  return (
    <AuthContext.Provider
      value={{
        user,
        accessToken,
        isLoading,
        loginWithGoogle,
        loginWithEmail,
        logout,
        updateTimezone,
        refreshSession,
      }}
    >
      {children}
    </AuthContext.Provider>
  );
}

export function useAuth() {
  const context = useContext(AuthContext);
  if (!context) {
    throw new Error("useAuth must be used within an AuthProvider");
  }
  return context;
}

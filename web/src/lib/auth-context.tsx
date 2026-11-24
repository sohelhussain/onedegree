"use client";

import React, { createContext, useContext, useEffect, useState } from "react";
import { User, AuthResponse } from "./types";
import { apiClient, setAccessToken } from "./api";
import { toast } from "sonner";

interface AuthContextType {
  user: User | null;
  accessToken: string | null;
  isLoading: boolean;
  loginWithGoogle: (idToken: string) => Promise<void>;
  logout: () => Promise<void>;
  updateTimezone: (timezone: string) => Promise<void>;
  refreshSession: () => Promise<boolean>;
}

const AuthContext = createContext<AuthContextType | undefined>(undefined);

export function AuthProvider({ children }: { children: React.ReactNode }) {
  const [user, setUser] = useState<User | null>(null);
  const [accessToken, setToken] = useState<string | null>(null);
  const [isLoading, setIsLoading] = useState<boolean>(true);

  const refreshSession = async (): Promise<boolean> => {
    try {
      // 1. Attempt token refresh using httpOnly cookie
      const refreshRes = await apiClient<{ access_token: string; expires_in: number }>(
        "/auth/refresh",
        { method: "POST" }
      );

      setAccessToken(refreshRes.access_token);
      setToken(refreshRes.access_token);

      // 2. Fetch authenticated profile
      const me = await apiClient<User>("/me");
      setUser(me);
      return true;
    } catch {
      setAccessToken(null);
      setToken(null);
      setUser(null);
      return false;
    } finally {
      setIsLoading(false);
    }
  };

  useEffect(() => {
    refreshSession();
  }, []);

  const loginWithGoogle = async (idToken: string) => {
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
  };

  const logout = async () => {
    try {
      await apiClient("/auth/logout", { method: "POST" });
    } catch {
      // continue even if logout endpoint errored
    } finally {
      setAccessToken(null);
      setToken(null);
      setUser(null);
      toast.info("Logged out");
    }
  };

  const updateTimezone = async (timezone: string) => {
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
  };

  return (
    <AuthContext.Provider
      value={{
        user,
        accessToken,
        isLoading,
        loginWithGoogle,
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

"use client";

import { useAuth } from "@/lib/auth-context";
import { GoogleSignInButton } from "@/components/auth/google-sign-in-button";
import {
  Compass,
  Coins,
  Globe,
  LogOut,
  RefreshCw,
  User as UserIcon,
  CheckCircle2,
  ShieldCheck,
  ArrowUpRight,
} from "lucide-react";
import Image from "next/image";
import { useState } from "react";

const COMMON_TIMEZONES = [
  "UTC",
  "America/New_York",
  "America/Los_Angeles",
  "America/Chicago",
  "Europe/London",
  "Europe/Paris",
  "Asia/Kolkata",
  "Asia/Tokyo",
  "Asia/Singapore",
  "Australia/Sydney",
];

export default function Home() {
  const { user, isLoading, logout, updateTimezone, refreshSession } = useAuth();
  const [selectedTz, setSelectedTz] = useState(user?.timezone || "UTC");
  const [isUpdatingTz, setIsUpdatingTz] = useState(false);

  const handleTimezoneChange = async (e: React.ChangeEvent<HTMLSelectElement>) => {
    const newTz = e.target.value;
    setSelectedTz(newTz);
    setIsUpdatingTz(true);
    try {
      await updateTimezone(newTz);
    } finally {
      setIsUpdatingTz(false);
    }
  };

  return (
    <main className="flex min-h-screen flex-col items-center justify-between p-6 md:p-16 selection:bg-emerald-500/30 selection:text-emerald-300">
      {/* Top Navbar */}
      <header className="w-full max-w-5xl flex items-center justify-between border-b border-zinc-800 pb-5">
        <div className="flex items-center gap-3">
          <div className="flex h-10 w-10 items-center justify-center rounded-xl bg-gradient-to-br from-emerald-500 to-teal-700 text-white shadow-lg shadow-emerald-500/20">
            <Compass className="h-5 w-5 rotate-12" />
          </div>
          <div>
            <span className="font-semibold text-lg tracking-tight">1degree</span>
            <span className="text-xs text-zinc-500 block font-mono">1degree.fit</span>
          </div>
        </div>

        <div className="flex items-center gap-4">
          <div className="flex items-center gap-1.5 px-3 py-1.5 rounded-full bg-amber-500/10 border border-amber-500/20 text-amber-400 text-sm font-medium">
            <Coins className="h-4 w-4" />
            <span>{user ? user.coin_balance : 0} coins</span>
          </div>

          {user ? (
            <div className="flex items-center gap-3">
              <div className="flex items-center gap-2 pl-2">
                {user.avatar_url ? (
                  <Image
                    src={user.avatar_url}
                    alt={user.full_name || "Avatar"}
                    width={32}
                    height={32}
                    className="h-8 w-8 rounded-full border border-zinc-700 object-cover"
                  />
                ) : (
                  <div className="h-8 w-8 rounded-full bg-zinc-800 border border-zinc-700 flex items-center justify-center text-zinc-300">
                    <UserIcon className="h-4 w-4" />
                  </div>
                )}
                <span className="text-sm font-medium text-zinc-200 hidden sm:inline">
                  {user.full_name || user.email || "User"}
                </span>
              </div>
              <button
                onClick={() => logout()}
                title="Sign out"
                className="p-2 rounded-lg bg-zinc-900 border border-zinc-800 text-zinc-400 hover:text-zinc-200 hover:border-zinc-700 transition"
              >
                <LogOut className="h-4 w-4" />
              </button>
            </div>
          ) : (
            <a
              href="http://localhost:8080/v1/swagger-ui"
              target="_blank"
              rel="noopener noreferrer"
              className="text-xs font-mono px-3 py-1.5 rounded-md bg-zinc-900 border border-zinc-800 text-zinc-400 hover:text-zinc-200 hover:border-zinc-700 transition"
            >
              Swagger Docs ↗
            </a>
          )}
        </div>
      </header>

      {/* Main Content Area */}
      <div className="my-auto max-w-3xl w-full py-8 space-y-8">
        {isLoading ? (
          <div className="flex flex-col items-center justify-center py-20 gap-3 text-zinc-400">
            <RefreshCw className="h-6 w-6 animate-spin text-emerald-400" />
            <p className="text-sm font-mono">Restoring session via httpOnly cookie...</p>
          </div>
        ) : user ? (
          /* Authenticated Dashboard View */
          <div className="space-y-6">
            <div className="p-6 rounded-2xl bg-zinc-900/60 border border-zinc-800 space-y-6">
              <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-4 border-b border-zinc-800/80 pb-5">
                <div className="flex items-center gap-3">
                  <div className="p-2.5 rounded-xl bg-emerald-500/10 border border-emerald-500/20 text-emerald-400">
                    <ShieldCheck className="h-6 w-6" />
                  </div>
                  <div>
                    <h2 className="text-lg font-semibold text-white">Google Authenticated</h2>
                    <p className="text-xs text-zinc-400 font-mono">
                      User ID: {user.id}
                    </p>
                  </div>
                </div>

                <button
                  onClick={() => refreshSession()}
                  className="inline-flex items-center gap-2 px-3 py-1.5 rounded-lg bg-zinc-800 hover:bg-zinc-700 text-zinc-300 text-xs font-medium transition"
                >
                  <RefreshCw className="h-3.5 w-3.5" />
                  Test Token Rotation
                </button>
              </div>

              {/* User Profile Attributes */}
              <div className="grid grid-cols-1 sm:grid-cols-2 gap-4 text-sm">
                <div className="p-4 rounded-xl bg-zinc-950/50 border border-zinc-800/60 space-y-1">
                  <span className="text-xs text-zinc-500 block">Email (Nullable / Unlinked)</span>
                  <span className="font-mono text-zinc-200">
                    {user.email || "No email provided"}
                  </span>
                </div>

                <div className="p-4 rounded-xl bg-zinc-950/50 border border-zinc-800/60 space-y-1">
                  <span className="text-xs text-zinc-500 block">Coin Balance</span>
                  <span className="font-semibold text-amber-400 flex items-center gap-1.5">
                    <Coins className="h-4 w-4" />
                    {user.coin_balance} coins
                  </span>
                </div>

                <div className="p-4 rounded-xl bg-zinc-950/50 border border-zinc-800/60 space-y-2 sm:col-span-2">
                  <div className="flex items-center justify-between">
                    <span className="text-xs text-zinc-500 flex items-center gap-1.5">
                      <Globe className="h-3.5 w-3.5" />
                      User Timezone (Governs 7-day habits & day boundaries)
                    </span>
                    {isUpdatingTz && (
                      <span className="text-xs text-emerald-400 animate-pulse">Saving...</span>
                    )}
                  </div>
                  <div className="flex gap-2">
                    <select
                      value={selectedTz}
                      onChange={handleTimezoneChange}
                      className="flex-1 bg-zinc-900 border border-zinc-700 rounded-lg px-3 py-2 text-sm text-zinc-200 focus:outline-none focus:border-emerald-500"
                    >
                      {COMMON_TIMEZONES.map((tz) => (
                        <option key={tz} value={tz}>
                          {tz}
                        </option>
                      ))}
                    </select>
                  </div>
                </div>
              </div>
            </div>

            {/* Next Steps Card */}
            <div className="p-6 rounded-2xl bg-emerald-950/20 border border-emerald-500/20 text-left space-y-2">
              <div className="flex items-center gap-2 text-emerald-400 font-semibold text-sm">
                <CheckCircle2 className="h-4 w-4" />
                Phase 2 Google Auth Complete
              </div>
              <p className="text-xs text-zinc-400 leading-relaxed">
                Tokens are secured via short-lived 15-minute JWTs and rotating hashed refresh tokens stored in <code className="text-zinc-300 font-mono">httpOnly</code> cookies. Replay detection revokes all sessions on reuse attempt.
              </p>
            </div>
          </div>
        ) : (
          /* Unauthenticated Welcome View */
          <div className="text-center space-y-8">
            <div className="space-y-4">
              <h1 className="text-4xl md:text-5xl font-bold tracking-tight text-white leading-tight">
                Small daily actions compound into <span className="text-emerald-400">massive transformation</span>.
              </h1>
              <p className="text-base md:text-lg text-zinc-400 leading-relaxed max-w-xl mx-auto">
                Sign in with Google to start tracking habits, earn coins, and manage daily micro-actions.
              </p>
            </div>

            {/* Google Sign In Component */}
            <div className="pt-2">
              <GoogleSignInButton />
            </div>

            {/* Feature Checklist */}
            <div className="grid grid-cols-1 md:grid-cols-3 gap-4 pt-6 text-left">
              <div className="p-4 rounded-xl bg-zinc-900/40 border border-zinc-800/80 space-y-1">
                <div className="text-emerald-400 text-xs font-medium">Google Auth First</div>
                <p className="text-xs text-zinc-500">
                  Strict JWKS verification checking iss, aud, and exp.
                </p>
              </div>

              <div className="p-4 rounded-xl bg-zinc-900/40 border border-zinc-800/80 space-y-1">
                <div className="text-emerald-400 text-xs font-medium">httpOnly Cookie</div>
                <p className="text-xs text-zinc-500">
                  Rotating refresh token stored hashed; automatic replay defense.
                </p>
              </div>

              <div className="p-4 rounded-xl bg-zinc-900/40 border border-zinc-800/80 space-y-1">
                <div className="text-emerald-400 text-xs font-medium">Timezone Aware</div>
                <p className="text-xs text-zinc-500">
                  Stored per user to govern midnight boundaries & 7-day grids.
                </p>
              </div>
            </div>
          </div>
        )}
      </div>

      {/* Footer */}
      <footer className="w-full max-w-5xl border-t border-zinc-800/80 pt-5 flex flex-col sm:flex-row items-center justify-between text-xs text-zinc-500 gap-2">
        <p>Phase 2 Complete • Google Auth, Token Rotation & Replay Defense Active</p>
        <a
          href="http://localhost:8080/v1/swagger-ui"
          target="_blank"
          rel="noopener noreferrer"
          className="hover:text-zinc-300 transition flex items-center gap-1 font-mono"
        >
          API Docs
          <ArrowUpRight className="h-3 w-3" />
        </a>
      </footer>
    </main>
  );
}

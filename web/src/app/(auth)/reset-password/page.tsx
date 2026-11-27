"use client";

import { Suspense, useEffect, useState } from "react";
import Link from "next/link";
import { useSearchParams } from "next/navigation";
import { resetPasswordApi } from "@/lib/api";
import { LogoMark } from "@/components/brand/logo-mark";
import { AlertCircle, CheckCircle2, Loader2 } from "lucide-react";
import { toast } from "sonner";

function ResetPasswordView() {
  const searchParams = useSearchParams();
  const [token] = useState(() => searchParams.get("token")?.trim() || "");
  const [newPassword, setNewPassword] = useState("");
  const [confirmPassword, setConfirmPassword] = useState("");
  const [submitting, setSubmitting] = useState(false);
  const [doneMessage, setDoneMessage] = useState<string | null>(null);
  const [errorMessage, setErrorMessage] = useState<string | null>(null);

  useEffect(() => {
    if (window.location.search) {
      window.history.replaceState(null, "", window.location.pathname);
    }
  }, []);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    setErrorMessage(null);

    if (!token) {
      setErrorMessage("Reset token is missing or invalid.");
      return;
    }
    if (newPassword.length < 8) {
      setErrorMessage("Password must be at least 8 characters long.");
      return;
    }
    if (newPassword !== confirmPassword) {
      setErrorMessage("Passwords do not match.");
      return;
    }

    setSubmitting(true);
    try {
      const res = await resetPasswordApi({ token, new_password: newPassword });
      setDoneMessage(res.message);
      toast.success("Password updated. Please sign in.");
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : "Reset link is invalid or expired.";
      setErrorMessage(msg);
      toast.error(msg);
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <main className="min-h-dvh w-full flex flex-col items-center justify-center px-4 py-8 safe-pt safe-pb bg-[var(--bg-canvas)]">
      <div className="w-full max-w-[400px] space-y-6">
        <div className="flex flex-col items-center text-center space-y-3">
          <LogoMark size={48} />
          <h1 className="text-xl font-bold">Choose a new password</h1>
        </div>

        <section className="rounded-2xl bg-[var(--bg-surface)] border border-[var(--border-subtle)] p-5 space-y-4">
          {doneMessage ? (
            <div className="text-center space-y-4">
              <CheckCircle2 className="h-10 w-10 mx-auto text-[var(--accent-course)]" />
              <p className="text-sm text-[var(--text-primary)]" data-testid="reset-success">
                {doneMessage}
              </p>
              <Link
                href="/login"
                className="min-h-[44px] w-full inline-flex items-center justify-center rounded-xl bg-emerald-500 hover:bg-emerald-400 text-zinc-950 font-semibold text-sm transition"
              >
                Sign in
              </Link>
            </div>
          ) : (
            <form onSubmit={handleSubmit} className="space-y-4" noValidate>
              {!token && (
                <div
                  role="alert"
                  className="flex items-start gap-2 p-3 rounded-xl bg-red-500/10 border border-red-500/30 text-xs text-red-200"
                >
                  <AlertCircle className="h-4 w-4 shrink-0 mt-0.5 text-red-400" />
                  <span>Password reset link is missing a token.</span>
                </div>
              )}

              <div className="space-y-1.5">
                <label
                  htmlFor="reset-password"
                  className="block text-xs font-medium text-[var(--text-muted)]"
                >
                  New password (min 8 characters)
                </label>
                <input
                  id="reset-password"
                  name="new_password"
                  type="password"
                  autoComplete="new-password"
                  required
                  minLength={8}
                  value={newPassword}
                  onChange={(e) => setNewPassword(e.target.value)}
                  placeholder="Enter new password"
                  className="min-h-[44px] w-full px-3.5 rounded-xl bg-[var(--bg-canvas)] border border-[var(--border-subtle)] text-sm text-[var(--text-primary)] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-emerald-400"
                />
              </div>

              <div className="space-y-1.5">
                <label
                  htmlFor="reset-confirm"
                  className="block text-xs font-medium text-[var(--text-muted)]"
                >
                  Confirm new password
                </label>
                <input
                  id="reset-confirm"
                  name="confirm_password"
                  type="password"
                  autoComplete="new-password"
                  required
                  minLength={8}
                  value={confirmPassword}
                  onChange={(e) => setConfirmPassword(e.target.value)}
                  placeholder="Re-enter new password"
                  className="min-h-[44px] w-full px-3.5 rounded-xl bg-[var(--bg-canvas)] border border-[var(--border-subtle)] text-sm text-[var(--text-primary)] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-emerald-400"
                />
              </div>

              {errorMessage && (
                <div
                  role="alert"
                  data-testid="reset-error"
                  className="flex items-start gap-2 p-3 rounded-xl bg-red-500/10 border border-red-500/30 text-xs text-red-200"
                >
                  <AlertCircle className="h-4 w-4 shrink-0 mt-0.5 text-red-400" />
                  <span>{errorMessage}</span>
                </div>
              )}

              <button
                type="submit"
                disabled={submitting || !token}
                className="min-h-[44px] w-full inline-flex items-center justify-center gap-2 px-4 rounded-xl bg-emerald-500 hover:bg-emerald-400 text-zinc-950 font-semibold text-sm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-emerald-300 disabled:opacity-50 transition"
              >
                {submitting && <Loader2 className="h-4 w-4 animate-spin" />}
                <span>{submitting ? "Updating..." : "Update password"}</span>
              </button>
            </form>
          )}
        </section>
      </div>
    </main>
  );
}

export default function ResetPasswordPage() {
  return (
    <Suspense
      fallback={
        <main className="min-h-dvh flex items-center justify-center bg-[var(--bg-canvas)] text-[var(--text-muted)]">
          <Loader2 className="h-6 w-6 animate-spin text-[var(--accent-course)]" />
        </main>
      }
    >
      <ResetPasswordView />
    </Suspense>
  );
}

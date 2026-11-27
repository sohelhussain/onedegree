"use client";

import { Suspense, useEffect, useRef, useState } from "react";
import Link from "next/link";
import { useRouter, useSearchParams } from "next/navigation";
import { useAuth } from "@/lib/auth-context";
import { ApiError } from "@/lib/api";
import { isEmailAuthEnabled, sanitizeNextPath } from "@/lib/auth-utils";
import { LogoMark } from "@/components/brand/logo-mark";
import { GoogleSignInButton } from "@/components/auth/google-sign-in-button";
import { ResendVerificationButton } from "@/components/auth/resend-verification-card";
import { Loader2, AlertCircle, MailWarning } from "lucide-react";
import { toast } from "sonner";

function LoginView() {
  const { user, isLoading, loginWithEmail } = useAuth();
  const router = useRouter();
  const searchParams = useSearchParams();
  const nextPath = sanitizeNextPath(searchParams.get("next"));
  const emailAuthEnabled = isEmailAuthEnabled();

  const [tab, setTab] = useState<"google" | "email">("google");
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [submitting, setSubmitting] = useState(false);
  const submittingRef = useRef(false);
  const [errorMessage, setErrorMessage] = useState<string | null>(null);
  const [needsVerificationEmail, setNeedsVerificationEmail] = useState<string | null>(null);

  useEffect(() => {
    if (!isLoading && user) {
      router.replace(nextPath);
    }
  }, [isLoading, user, nextPath, router]);

  const handleEmailSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (submittingRef.current) return;
    setErrorMessage(null);
    setNeedsVerificationEmail(null);

    const submittedEmail = email.trim();
    const submittedPassword = password;
    setEmail("");
    setPassword("");

    if (!submittedEmail || !submittedPassword) {
      setErrorMessage("Please enter your email and password.");
      return;
    }

    submittingRef.current = true;
    setSubmitting(true);
    try {
      await loginWithEmail(submittedEmail, submittedPassword);
      router.replace(nextPath);
    } catch (err: unknown) {
      if (err instanceof ApiError && err.code === "EMAIL_NOT_VERIFIED") {
        setNeedsVerificationEmail(submittedEmail);
        setErrorMessage("Verify your email before signing in.");
        toast.error("Please verify your email before signing in.");
      } else {
        const msg = err instanceof Error ? err.message : "Invalid email or password";
        setErrorMessage(msg);
        toast.error(msg);
      }
    } finally {
      submittingRef.current = false;
      setSubmitting(false);
    }
  };

  if (isLoading || user) {
    return (
      <main className="min-h-dvh flex items-center justify-center bg-[var(--bg-canvas)] text-[var(--text-muted)]">
        <Loader2 className="h-6 w-6 animate-spin text-[var(--accent-course)]" />
      </main>
    );
  }

  return (
    <main className="min-h-dvh w-full flex flex-col items-center justify-center px-4 py-8 safe-pt safe-pb bg-[var(--bg-canvas)]">
      <div className="w-full max-w-[400px] space-y-6">
        <div className="flex flex-col items-center text-center space-y-3">
          <LogoMark size={56} />
          <div className="space-y-1">
            <h1 className="text-2xl font-bold tracking-tight text-[var(--text-primary)]">
              1degree
            </h1>
            <p className="text-sm text-[var(--text-muted)]">Stay 1° on course</p>
          </div>
        </div>

        <section className="rounded-2xl bg-[var(--bg-surface)] border border-[var(--border-subtle)] p-5 space-y-5 shadow-xl">
          {emailAuthEnabled && (
            <div
              role="tablist"
              aria-label="Sign in method"
              className="grid grid-cols-2 gap-1.5 p-1 rounded-xl bg-[var(--bg-canvas)] border border-[var(--border-subtle)]"
            >
              <button
                type="button"
                role="tab"
                aria-selected={tab === "google"}
                onClick={() => {
                  setTab("google");
                  setErrorMessage(null);
                  setNeedsVerificationEmail(null);
                }}
                className={`min-h-[44px] rounded-lg text-sm font-medium transition focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-emerald-400 ${
                  tab === "google"
                    ? "bg-[var(--bg-elevated)] text-[var(--text-primary)] shadow"
                    : "text-[var(--text-muted)] hover:text-[var(--text-primary)]"
                }`}
              >
                Google
              </button>
              <button
                type="button"
                role="tab"
                aria-selected={tab === "email"}
                onClick={() => {
                  setTab("email");
                  setErrorMessage(null);
                }}
                className={`min-h-[44px] rounded-lg text-sm font-medium transition focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-emerald-400 ${
                  tab === "email"
                    ? "bg-[var(--bg-elevated)] text-[var(--text-primary)] shadow"
                    : "text-[var(--text-muted)] hover:text-[var(--text-primary)]"
                }`}
              >
                Email
              </button>
            </div>
          )}

          <div className={emailAuthEnabled && tab !== "google" ? "hidden" : "space-y-3 py-2"}>
            <GoogleSignInButton onSuccess={() => router.replace(nextPath)} />
            <p className="text-xs text-center text-[var(--text-muted)]">
              Quick sign-in with your Google account
            </p>
          </div>

          {emailAuthEnabled && tab === "email" && (
            <form onSubmit={handleEmailSubmit} className="space-y-4" noValidate>
              <div className="space-y-1.5">
                <label
                  htmlFor="login-email"
                  className="block text-xs font-medium text-[var(--text-muted)]"
                >
                  Email
                </label>
                <input
                  id="login-email"
                  name="login_email"
                  type="email"
                  autoComplete="section-login username"
                  autoCorrect="off"
                  autoCapitalize="none"
                  spellCheck={false}
                  enterKeyHint="next"
                  required
                  value={email}
                  onChange={(e) => setEmail(e.target.value)}
                  placeholder="you@example.com"
                  className="min-h-[44px] w-full px-3.5 rounded-xl bg-[var(--bg-canvas)] border border-[var(--border-subtle)] text-sm text-[var(--text-primary)] placeholder:text-zinc-600 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-emerald-400"
                />
              </div>

              <div className="space-y-1.5">
                <div className="flex items-center justify-between">
                  <label
                    htmlFor="login-password"
                    className="block text-xs font-medium text-[var(--text-muted)]"
                  >
                    Password
                  </label>
                  <Link
                    href="/forgot-password"
                    className="text-xs text-emerald-400 hover:text-emerald-300 py-1 px-1 rounded focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-emerald-400"
                  >
                    Forgot password?
                  </Link>
                </div>
                <input
                  id="login-password"
                  name="login_password"
                  type="password"
                  autoComplete="section-login current-password"
                  enterKeyHint="done"
                  required
                  value={password}
                  onChange={(e) => setPassword(e.target.value)}
                  placeholder="Enter your password"
                  className="min-h-[44px] w-full px-3.5 rounded-xl bg-[var(--bg-canvas)] border border-[var(--border-subtle)] text-sm text-[var(--text-primary)] placeholder:text-zinc-600 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-emerald-400"
                />
              </div>

              {errorMessage && !needsVerificationEmail && (
                <div
                  role="alert"
                  data-testid="login-error"
                  className="flex items-start gap-2 p-3 rounded-xl bg-red-500/10 border border-red-500/30 text-xs text-red-200"
                >
                  <AlertCircle className="h-4 w-4 shrink-0 mt-0.5 text-red-400" />
                  <span>{errorMessage}</span>
                </div>
              )}

              {needsVerificationEmail && (
                <div
                  role="alert"
                  data-testid="verify-email-notice"
                  className="p-3.5 rounded-xl bg-amber-500/10 border border-amber-500/30 space-y-3"
                >
                  <div className="flex items-start gap-2 text-xs text-amber-200">
                    <MailWarning className="h-4 w-4 shrink-0 mt-0.5 text-[var(--coin-gold)]" />
                    <div>
                      <p className="font-semibold text-[var(--coin-gold)]">Verify your email</p>
                      <p className="text-amber-200/80 mt-0.5">
                        Your account is not verified yet. Check your inbox or request a fresh link.
                      </p>
                    </div>
                  </div>
                  <ResendVerificationButton
                    email={needsVerificationEmail}
                    label="Resend verification"
                  />
                </div>
              )}

              <button
                type="submit"
                disabled={submitting}
                className="min-h-[44px] w-full inline-flex items-center justify-center gap-2 px-4 rounded-xl bg-emerald-500 hover:bg-emerald-400 text-zinc-950 font-semibold text-sm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-emerald-300 disabled:opacity-60 transition"
              >
                {submitting && <Loader2 className="h-4 w-4 animate-spin" />}
                <span>{submitting ? "Signing in..." : "Sign in"}</span>
              </button>

              <p className="text-center text-xs text-[var(--text-muted)] pt-1">
                New to 1degree?{" "}
                <Link
                  href="/register"
                  className="text-emerald-400 hover:text-emerald-300 font-medium py-1 px-1 rounded focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-emerald-400"
                >
                  Create an account
                </Link>
              </p>
            </form>
          )}
        </section>
      </div>
    </main>
  );
}

export default function LoginPage() {
  return (
    <Suspense
      fallback={
        <main className="min-h-dvh flex items-center justify-center bg-[var(--bg-canvas)] text-[var(--text-muted)]">
          <Loader2 className="h-6 w-6 animate-spin text-[var(--accent-course)]" />
        </main>
      }
    >
      <LoginView />
    </Suspense>
  );
}

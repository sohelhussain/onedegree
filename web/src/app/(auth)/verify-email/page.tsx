"use client";

import { Suspense, useEffect, useRef, useState } from "react";
import Link from "next/link";
import { useRouter, useSearchParams } from "next/navigation";
import { useAuth } from "@/lib/auth-context";
import { AlertCircle, CheckCircle2, Loader2, ShieldCheck, XCircle } from "lucide-react";

function VerifyEmailView() {
  const router = useRouter();
  const { verifyEmailAndLogin } = useAuth();
  const searchParams = useSearchParams();
  const initialToken = searchParams.get("token");
  const passwordInputRef = useRef<HTMLInputElement>(null);
  const submittingRef = useRef(false);

  const [token] = useState(() => initialToken?.trim() || "");
  const [password, setPassword] = useState("");
  const [submitting, setSubmitting] = useState(false);
  const [status, setStatus] = useState<"prompt" | "success" | "error">(() =>
    initialToken?.trim() ? "prompt" : "error"
  );
  const [errorMessage, setErrorMessage] = useState<string | null>(null);
  const [message] = useState(() =>
    initialToken?.trim() ? "" : "Verification link is missing or incomplete."
  );

  useEffect(() => {
    // Strip the secret token from the address bar immediately
    if (window.location.search) {
      window.history.replaceState(null, "", window.location.pathname);
    }
  }, []);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (submittingRef.current || !token) return;
    setErrorMessage(null);

    const submittedPassword = password;
    setPassword("");

    if (!submittedPassword) {
      setErrorMessage("Please enter your sign-up password to activate your account.");
      passwordInputRef.current?.focus();
      return;
    }

    submittingRef.current = true;
    setSubmitting(true);
    try {
      await verifyEmailAndLogin(token, submittedPassword);
      setStatus("success");
      router.replace("/today");
    } catch (err: unknown) {
      const msg =
        err instanceof Error
          ? err.message
          : "This verification link is invalid or has expired.";
      setErrorMessage(msg);
      passwordInputRef.current?.focus();
    } finally {
      submittingRef.current = false;
      setSubmitting(false);
    }
  };

  return (
    <main className="min-h-dvh w-full flex flex-col items-center justify-center px-4 py-8 safe-pt safe-pb bg-[var(--bg-canvas)]">
      <div className="w-full max-w-[400px] rounded-2xl bg-[var(--bg-surface)] border border-[var(--border-subtle)] p-6 text-center space-y-5">
        {status === "prompt" && (
          <>
            <div className="mx-auto h-12 w-12 rounded-2xl bg-emerald-500/15 border border-emerald-500/30 flex items-center justify-center text-[var(--accent-course)]">
              <ShieldCheck className="h-6 w-6" />
            </div>
            <div className="space-y-1.5">
              <h1 className="text-lg font-semibold text-[var(--text-primary)]">
                Confirm your email
              </h1>
              <p className="text-sm text-[var(--text-muted)]">
                Enter the password you used to sign up to activate your account.
              </p>
            </div>

            <form onSubmit={handleSubmit} className="space-y-4 text-left" noValidate>
              <div className="space-y-1.5">
                <label
                  htmlFor="verify-password"
                  className="block text-xs font-medium text-[var(--text-muted)]"
                >
                  Password
                </label>
                <input
                  ref={passwordInputRef}
                  id="verify-password"
                  name="verify_password"
                  type="password"
                  autoComplete="section-register current-password"
                  enterKeyHint="done"
                  required
                  minLength={8}
                  aria-invalid={Boolean(errorMessage)}
                  aria-describedby={errorMessage ? "verify-password-error" : undefined}
                  value={password}
                  onChange={(e) => {
                    setPassword(e.target.value);
                    if (errorMessage) setErrorMessage(null);
                  }}
                  placeholder="Enter your sign-up password"
                  className="min-h-[48px] w-full px-3.5 rounded-xl bg-[var(--bg-canvas)] border border-[var(--border-subtle)] text-base sm:text-sm text-[var(--text-primary)] placeholder:text-zinc-600 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-emerald-400"
                />
              </div>

              {errorMessage && (
                <div
                  id="verify-password-error"
                  role="alert"
                  data-testid="verify-error"
                  className="flex items-start gap-2 p-3 rounded-xl bg-red-500/10 border border-red-500/30 text-xs text-red-200"
                >
                  <AlertCircle className="h-4 w-4 shrink-0 mt-0.5 text-red-400" />
                  <span>{errorMessage}</span>
                </div>
              )}

              <button
                type="submit"
                disabled={submitting}
                className="min-h-[48px] w-full rounded-xl bg-emerald-500 hover:bg-emerald-400 disabled:opacity-60 text-zinc-950 font-semibold text-sm flex items-center justify-center gap-2 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-emerald-300 transition"
              >
                {submitting && <Loader2 className="h-4 w-4 animate-spin" />}
                <span>Verify email</span>
              </button>
            </form>

            <Link
              href="/login"
              className="min-h-[44px] w-full inline-flex items-center justify-center rounded-xl text-xs font-medium text-[var(--text-muted)] hover:text-[var(--text-primary)] transition"
            >
              Back to sign in
            </Link>
          </>
        )}

        {status === "success" && (
          <>
            <CheckCircle2 className="h-10 w-10 mx-auto text-[var(--accent-course)]" />
            <h1 className="text-lg font-semibold" data-testid="verify-success-heading">
              Email verified
            </h1>
            <p className="text-sm text-[var(--text-muted)]">Taking you to your dashboard...</p>
            <Link
              href="/today"
              className="min-h-[44px] w-full inline-flex items-center justify-center rounded-xl bg-emerald-500 hover:bg-emerald-400 text-zinc-950 font-semibold text-sm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-emerald-300 transition"
            >
              Continue to today
            </Link>
          </>
        )}

        {status === "error" && (
          <>
            <XCircle className="h-10 w-10 mx-auto text-red-400" />
            <h1 className="text-lg font-semibold">Verification failed</h1>
            <p className="text-sm text-[var(--text-muted)]" role="alert">
              {message}
            </p>
            <Link
              href="/login"
              className="min-h-[44px] w-full inline-flex items-center justify-center rounded-xl bg-[var(--bg-elevated)] border border-[var(--border-subtle)] text-sm font-medium text-[var(--text-primary)] hover:border-emerald-400/50 transition"
            >
              Back to sign in
            </Link>
          </>
        )}
      </div>
    </main>
  );
}

export default function VerifyEmailPage() {
  return (
    <Suspense
      fallback={
        <main className="min-h-dvh flex items-center justify-center bg-[var(--bg-canvas)] text-[var(--text-muted)]">
          <Loader2 className="h-6 w-6 animate-spin text-[var(--accent-course)]" />
        </main>
      }
    >
      <VerifyEmailView />
    </Suspense>
  );
}

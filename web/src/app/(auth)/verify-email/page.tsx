"use client";

import { Suspense, useEffect, useRef, useState } from "react";
import Link from "next/link";
import { useSearchParams } from "next/navigation";
import { verifyEmailApi } from "@/lib/api";
import { CheckCircle2, Loader2, XCircle } from "lucide-react";

function VerifyEmailView() {
  const searchParams = useSearchParams();
  const initialToken = searchParams.get("token");
  const submittedRef = useRef(false);
  const [status, setStatus] = useState<"verifying" | "success" | "error">("verifying");
  const [message, setMessage] = useState("Verifying your email...");

  useEffect(() => {
    // Strip the secret token from the address bar immediately
    if (window.location.search) {
      window.history.replaceState(null, "", window.location.pathname);
    }

    if (submittedRef.current) return;
    submittedRef.current = true;

    const token = initialToken?.trim();
    if (!token) {
      Promise.resolve().then(() => {
        setStatus("error");
        setMessage("Verification link is missing or incomplete.");
      });
      return;
    }

    verifyEmailApi(token)
      .then((res) => {
        setStatus("success");
        setMessage(res.message || "Email verified! You can now sign in.");
      })
      .catch((err: unknown) => {
        setStatus("error");
        setMessage(
          err instanceof Error
            ? err.message
            : "This verification link is invalid or has expired."
        );
      });
  }, [initialToken]);

  return (
    <main className="min-h-dvh w-full flex flex-col items-center justify-center px-4 py-8 safe-pt safe-pb bg-[var(--bg-canvas)]">
      <div className="w-full max-w-[400px] rounded-2xl bg-[var(--bg-surface)] border border-[var(--border-subtle)] p-6 text-center space-y-5">
        {status === "verifying" && (
          <>
            <Loader2 className="h-10 w-10 mx-auto animate-spin text-[var(--accent-course)]" />
            <h1 className="text-lg font-semibold">Verifying email</h1>
            <p className="text-sm text-[var(--text-muted)]">{message}</p>
          </>
        )}

        {status === "success" && (
          <>
            <CheckCircle2 className="h-10 w-10 mx-auto text-[var(--accent-course)]" />
            <h1 className="text-lg font-semibold" data-testid="verify-success-heading">
              Email verified
            </h1>
            <p className="text-sm text-[var(--text-muted)]">{message}</p>
            <Link
              href="/login"
              className="min-h-[44px] w-full inline-flex items-center justify-center rounded-xl bg-emerald-500 hover:bg-emerald-400 text-zinc-950 font-semibold text-sm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-emerald-300 transition"
            >
              Continue to sign in
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

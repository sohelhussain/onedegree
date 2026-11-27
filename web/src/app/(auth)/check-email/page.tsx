"use client";

import { Suspense, useState } from "react";
import Link from "next/link";
import { useSearchParams } from "next/navigation";
import { isEmailAuthEnabled } from "@/lib/auth-utils";
import { LogoMark } from "@/components/brand/logo-mark";
import { ResendVerificationButton } from "@/components/auth/resend-verification-card";
import { ArrowLeft, Loader2, Mail } from "lucide-react";

function CheckEmailView() {
  const searchParams = useSearchParams();
  const initialEmail = searchParams.get("email") || "";
  const [email, setEmail] = useState(initialEmail);

  if (!isEmailAuthEnabled()) {
    return (
      <main className="min-h-dvh flex items-center justify-center px-4 bg-[var(--bg-canvas)]">
        <div className="w-full max-w-[400px] rounded-2xl bg-[var(--bg-surface)] border border-[var(--border-subtle)] p-6 text-center space-y-4">
          <LogoMark size={44} />
          <p className="text-sm text-[var(--text-muted)]">Email sign-in is disabled.</p>
          <Link
            href="/login"
            className="min-h-[44px] inline-flex items-center justify-center px-5 rounded-xl bg-emerald-500 text-zinc-950 font-semibold text-sm"
          >
            Go to login
          </Link>
        </div>
      </main>
    );
  }

  return (
    <main className="min-h-dvh w-full flex flex-col items-center justify-center px-4 py-8 safe-pt safe-pb bg-[var(--bg-canvas)]">
      <div className="w-full max-w-[400px] rounded-2xl bg-[var(--bg-surface)] border border-[var(--border-subtle)] p-6 space-y-5 text-center">
        <div className="mx-auto h-12 w-12 rounded-2xl bg-emerald-500/15 border border-emerald-500/30 flex items-center justify-center text-[var(--accent-course)]">
          <Mail className="h-6 w-6" />
        </div>

        <div className="space-y-1.5">
          <h1 className="text-xl font-bold text-[var(--text-primary)]">Check your email</h1>
          <p className="text-sm text-[var(--text-muted)]">
            We sent a verification link{email ? ` to ${email}` : ""}. Open the link to activate your
            account.
          </p>
        </div>

        <div className="space-y-3 text-left">
          {!initialEmail && (
            <div className="space-y-1.5">
              <label
                htmlFor="resend-email-input"
                className="block text-xs font-medium text-[var(--text-muted)]"
              >
                Email address
              </label>
              <input
                id="resend-email-input"
                type="email"
                value={email}
                onChange={(e) => setEmail(e.target.value)}
                placeholder="you@example.com"
                className="min-h-[44px] w-full px-3.5 rounded-xl bg-[var(--bg-canvas)] border border-[var(--border-subtle)] text-sm text-[var(--text-primary)]"
              />
            </div>
          )}

          <ResendVerificationButton
            email={email}
            initialCooldown={initialEmail ? 60 : 0}
          />
        </div>

        <Link
          href="/login"
          className="min-h-[44px] w-full inline-flex items-center justify-center gap-2 text-xs text-[var(--text-muted)] hover:text-[var(--text-primary)] rounded-xl focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-emerald-400"
        >
          <ArrowLeft className="h-3.5 w-3.5" />
          <span>Back to sign in</span>
        </Link>
      </div>
    </main>
  );
}

export default function CheckEmailPage() {
  return (
    <Suspense
      fallback={
        <main className="min-h-dvh flex items-center justify-center bg-[var(--bg-canvas)] text-[var(--text-muted)]">
          <Loader2 className="h-6 w-6 animate-spin text-[var(--accent-course)]" />
        </main>
      }
    >
      <CheckEmailView />
    </Suspense>
  );
}

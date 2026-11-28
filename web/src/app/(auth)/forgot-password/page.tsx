"use client";

import { useState } from "react";
import Link from "next/link";
import { forgotPasswordApi } from "@/lib/api";
import { isEmailAuthEnabled } from "@/lib/auth-utils";
import { LogoMark } from "@/components/brand/logo-mark";
import { ArrowLeft, CheckCircle2, Loader2 } from "lucide-react";
import { toast } from "sonner";

export default function ForgotPasswordPage() {
  const enabled = isEmailAuthEnabled();
  const [email, setEmail] = useState("");
  const [submitting, setSubmitting] = useState(false);
  const [sentMessage, setSentMessage] = useState<string | null>(null);

  if (!enabled) {
    return (
      <main className="min-h-dvh flex items-center justify-center px-4 bg-[var(--bg-canvas)]">
        <div className="w-full max-w-[400px] rounded-2xl bg-[var(--bg-surface)] border border-[var(--border-subtle)] p-6 text-center space-y-4">
          <LogoMark size={44} />
          <p className="text-sm text-[var(--text-muted)]">Password sign-in is disabled.</p>
          <Link
            href="/login"
            className="min-h-[44px] inline-flex items-center justify-center px-5 rounded-xl bg-emerald-500 text-zinc-950 font-semibold text-sm"
          >
            Back to sign in
          </Link>
        </div>
      </main>
    );
  }

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    const submittedEmail = email.trim();
    setEmail("");
    if (!submittedEmail) return;
    setSubmitting(true);
    try {
      const res = await forgotPasswordApi(submittedEmail);
      setSentMessage(res.message);
      toast.success("If registered, reset instructions were sent.");
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : "Could not request password reset";
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
          <h1 className="text-xl font-bold">Reset password</h1>
          <p className="text-sm text-[var(--text-muted)]">
            Enter your email to receive a password reset link.
          </p>
        </div>

        <section className="rounded-2xl bg-[var(--bg-surface)] border border-[var(--border-subtle)] p-5 space-y-4">
          {sentMessage ? (
            <div className="text-center space-y-4 py-2">
              <CheckCircle2 className="h-9 w-9 mx-auto text-[var(--accent-course)]" />
              <p className="text-sm text-[var(--text-primary)]" data-testid="forgot-confirmation">
                {sentMessage}
              </p>
            </div>
          ) : (
            <form onSubmit={handleSubmit} className="space-y-4">
              <div className="space-y-1.5">
                <label
                  htmlFor="forgot-email"
                  className="block text-xs font-medium text-[var(--text-muted)]"
                >
                  Email
                </label>
                <input
                  id="forgot-email"
                  name="forgot_email"
                  type="email"
                  autoComplete="email"
                  autoCorrect="off"
                  autoCapitalize="none"
                  spellCheck={false}
                  enterKeyHint="done"
                  required
                  value={email}
                  onChange={(e) => setEmail(e.target.value)}
                  placeholder="you@example.com"
                  className="min-h-[44px] w-full px-3.5 rounded-xl bg-[var(--bg-canvas)] border border-[var(--border-subtle)] text-sm text-[var(--text-primary)] placeholder:text-zinc-600 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-emerald-400"
                />
              </div>

              <button
                type="submit"
                disabled={submitting}
                className="min-h-[44px] w-full inline-flex items-center justify-center gap-2 px-4 rounded-xl bg-emerald-500 hover:bg-emerald-400 text-zinc-950 font-semibold text-sm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-emerald-300 disabled:opacity-60 transition"
              >
                {submitting && <Loader2 className="h-4 w-4 animate-spin" />}
                <span>{submitting ? "Sending..." : "Send reset link"}</span>
              </button>
            </form>
          )}

          <Link
            href="/login"
            className="min-h-[44px] w-full inline-flex items-center justify-center gap-2 text-xs text-[var(--text-muted)] hover:text-[var(--text-primary)] rounded-xl focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-emerald-400"
          >
            <ArrowLeft className="h-3.5 w-3.5" />
            <span>Back to sign in</span>
          </Link>
        </section>
      </div>
    </main>
  );
}

"use client";

import { useState } from "react";
import Link from "next/link";
import { useRouter } from "next/navigation";
import { registerAccount } from "@/lib/api";
import { isEmailAuthEnabled } from "@/lib/auth-utils";
import { LogoMark } from "@/components/brand/logo-mark";
import { AlertCircle, ArrowLeft, Loader2 } from "lucide-react";
import { toast } from "sonner";

export default function RegisterPage() {
  const router = useRouter();
  const enabled = isEmailAuthEnabled();
  const [fullName, setFullName] = useState("");
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [submitting, setSubmitting] = useState(false);
  const [errorMessage, setErrorMessage] = useState<string | null>(null);

  if (!enabled) {
    return (
      <main className="min-h-dvh flex items-center justify-center px-4 bg-[var(--bg-canvas)]">
        <div className="w-full max-w-[400px] rounded-2xl bg-[var(--bg-surface)] border border-[var(--border-subtle)] p-6 text-center space-y-4">
          <LogoMark size={44} />
          <h1 className="text-lg font-semibold">Email registration is disabled</h1>
          <p className="text-sm text-[var(--text-muted)]">
            Please continue with Google Sign-In on the login page.
          </p>
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
    setErrorMessage(null);

    if (password.length < 8) {
      setErrorMessage("Password must be at least 8 characters long.");
      return;
    }

    setSubmitting(true);
    try {
      const trimmedEmail = email.trim();
      await registerAccount({
        email: trimmedEmail,
        password,
        full_name: fullName.trim() || undefined,
      });
      toast.success("Account created! Check your email to verify.");
      router.push(`/check-email?email=${encodeURIComponent(trimmedEmail)}`);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : "Could not create account";
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
          <div className="space-y-1">
            <h1 className="text-xl font-bold tracking-tight">Create your account</h1>
            <p className="text-sm text-[var(--text-muted)]">Stay 1° on course</p>
          </div>
        </div>

        <section className="rounded-2xl bg-[var(--bg-surface)] border border-[var(--border-subtle)] p-5 space-y-4">
          <form onSubmit={handleSubmit} className="space-y-4" noValidate>
            <div className="space-y-1.5">
              <label
                htmlFor="reg-name"
                className="block text-xs font-medium text-[var(--text-muted)]"
              >
                Name (optional)
              </label>
              <input
                id="reg-name"
                name="full_name"
                type="text"
                autoComplete="name"
                value={fullName}
                onChange={(e) => setFullName(e.target.value)}
                placeholder="Alex Rivera"
                className="min-h-[44px] w-full px-3.5 rounded-xl bg-[var(--bg-canvas)] border border-[var(--border-subtle)] text-sm text-[var(--text-primary)] placeholder:text-zinc-600 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-emerald-400"
              />
            </div>

            <div className="space-y-1.5">
              <label
                htmlFor="reg-email"
                className="block text-xs font-medium text-[var(--text-muted)]"
              >
                Email
              </label>
              <input
                id="reg-email"
                name="email"
                type="email"
                autoComplete="email"
                required
                value={email}
                onChange={(e) => setEmail(e.target.value)}
                placeholder="you@example.com"
                className="min-h-[44px] w-full px-3.5 rounded-xl bg-[var(--bg-canvas)] border border-[var(--border-subtle)] text-sm text-[var(--text-primary)] placeholder:text-zinc-600 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-emerald-400"
              />
            </div>

            <div className="space-y-1.5">
              <label
                htmlFor="reg-password"
                className="block text-xs font-medium text-[var(--text-muted)]"
              >
                Password (min 8 characters)
              </label>
              <input
                id="reg-password"
                name="password"
                type="password"
                autoComplete="new-password"
                required
                minLength={8}
                value={password}
                onChange={(e) => setPassword(e.target.value)}
                placeholder="Create a strong password"
                className="min-h-[44px] w-full px-3.5 rounded-xl bg-[var(--bg-canvas)] border border-[var(--border-subtle)] text-sm text-[var(--text-primary)] placeholder:text-zinc-600 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-emerald-400"
              />
            </div>

            {errorMessage && (
              <div
                role="alert"
                className="flex items-start gap-2 p-3 rounded-xl bg-red-500/10 border border-red-500/30 text-xs text-red-200"
              >
                <AlertCircle className="h-4 w-4 shrink-0 mt-0.5 text-red-400" />
                <span>{errorMessage}</span>
              </div>
            )}

            <button
              type="submit"
              disabled={submitting}
              className="min-h-[44px] w-full inline-flex items-center justify-center gap-2 px-4 rounded-xl bg-emerald-500 hover:bg-emerald-400 text-zinc-950 font-semibold text-sm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-emerald-300 disabled:opacity-60 transition"
            >
              {submitting && <Loader2 className="h-4 w-4 animate-spin" />}
              <span>{submitting ? "Creating account..." : "Create account"}</span>
            </button>
          </form>

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

"use client";

import { useEffect, useRef, useState } from "react";
import { resendVerificationApi } from "@/lib/api";
import { toast } from "sonner";
import { Loader2, MailCheck } from "lucide-react";

const COOLDOWN_SECONDS = 60;

export function ResendVerificationButton({
  email,
  initialCooldown = 0,
  label = "Resend verification email",
}: {
  email: string;
  initialCooldown?: number;
  label?: string;
}) {
  const [cooldown, setCooldown] = useState(initialCooldown);
  const [isSending, setIsSending] = useState(false);
  const sendingRef = useRef(false);

  useEffect(() => {
    if (cooldown <= 0) return;
    const timer = window.setInterval(() => {
      setCooldown((prev) => (prev <= 1 ? 0 : prev - 1));
    }, 1000);
    return () => window.clearInterval(timer);
  }, [cooldown]);

  const handleResend = async () => {
    const trimmed = email.trim();
    if (!trimmed || cooldown > 0 || isSending || sendingRef.current) return;
    sendingRef.current = true;
    setIsSending(true);
    try {
      const res = await resendVerificationApi(trimmed);
      toast.success(res.message);
      setCooldown(COOLDOWN_SECONDS);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : "Could not resend verification email";
      toast.error(msg);
    } finally {
      sendingRef.current = false;
      setIsSending(false);
    }
  };

  return (
    <button
      type="button"
      onClick={handleResend}
      disabled={!email.trim() || cooldown > 0 || isSending}
      className="min-h-[44px] w-full inline-flex items-center justify-center gap-2 px-4 py-2.5 rounded-xl bg-[var(--bg-elevated)] border border-[var(--border-subtle)] text-sm font-medium text-[var(--text-primary)] hover:border-emerald-400/50 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-emerald-400 disabled:opacity-50 disabled:cursor-not-allowed transition"
    >
      {isSending ? (
        <Loader2 className="h-4 w-4 animate-spin" />
      ) : (
        <MailCheck className="h-4 w-4 text-[var(--accent-course)]" />
      )}
      <span>
        {cooldown > 0 ? `Resend available in ${cooldown}s` : label}
      </span>
    </button>
  );
}

"use client";

import { useAuth } from "@/lib/auth-context";
import { CoinIcon } from "@/components/brand/coin-icon";

export default function TodayPage() {
  const { user } = useAuth();
  const displayName = user?.full_name?.trim() || "Explorer";
  const coinBalance = user?.coin_balance ?? 0;

  return (
    <div className="space-y-5">
      <section className="rounded-2xl bg-[var(--bg-surface)] border border-[var(--border-subtle)] p-5 space-y-1.5">
        <p className="text-xs uppercase tracking-wider text-[var(--accent-course)] font-semibold">
          Today
        </p>
        <h1 className="text-xl font-bold text-[var(--text-primary)]" data-testid="today-greeting">
          Welcome, {displayName}
        </h1>
      </section>

      <section className="rounded-2xl bg-[var(--bg-surface)] border border-amber-500/30 p-5 flex items-center justify-between">
        <div className="space-y-1">
          <p className="text-xs uppercase tracking-wider text-[var(--text-muted)]">
            Coin Balance
          </p>
          <p
            className="font-game text-2xl text-[var(--coin-gold)] tabular-nums"
            data-testid="today-coin-balance"
          >
            {coinBalance}
          </p>
        </div>
        <div className="h-12 w-12 rounded-2xl bg-amber-500/10 border border-amber-500/25 flex items-center justify-center">
          <CoinIcon size={28} />
        </div>
      </section>
    </div>
  );
}

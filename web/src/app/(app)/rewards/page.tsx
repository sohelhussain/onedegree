import { Gift } from "lucide-react";

export default function RewardsPage() {
  return (
    <div className="rounded-2xl bg-[var(--bg-surface)] border border-[var(--border-subtle)] p-6 text-center space-y-3">
      <div className="mx-auto h-12 w-12 rounded-2xl bg-amber-500/10 border border-amber-500/25 flex items-center justify-center text-[var(--coin-gold)]">
        <Gift className="h-6 w-6" />
      </div>
      <h1 className="text-lg font-bold text-[var(--text-primary)]">Rewards</h1>
      <p className="text-sm text-[var(--text-muted)]">
        No rewards configured yet. Rewards and coin redemptions are coming soon.
      </p>
    </div>
  );
}

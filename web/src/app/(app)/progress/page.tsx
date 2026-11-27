import { LineChart } from "lucide-react";

export default function ProgressPage() {
  return (
    <div className="rounded-2xl bg-[var(--bg-surface)] border border-[var(--border-subtle)] p-6 text-center space-y-3">
      <div className="mx-auto h-12 w-12 rounded-2xl bg-emerald-500/10 border border-emerald-500/25 flex items-center justify-center text-[var(--accent-course)]">
        <LineChart className="h-6 w-6" />
      </div>
      <h1 className="text-lg font-bold text-[var(--text-primary)]">Progress</h1>
      <p className="text-sm text-[var(--text-muted)]">
        No habit history yet. Your streaks and 1° course progress will appear here soon.
      </p>
    </div>
  );
}

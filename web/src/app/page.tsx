"use client";

import { useEffect } from "react";
import { useRouter } from "next/navigation";
import { useAuth } from "@/lib/auth-context";
import { Loader2 } from "lucide-react";

export default function HomePage() {
  const { user, isLoading } = useAuth();
  const router = useRouter();

  useEffect(() => {
    if (isLoading) return;
    router.replace(user ? "/today" : "/login");
  }, [isLoading, user, router]);

  return (
    <main className="min-h-dvh flex flex-col items-center justify-center gap-3 bg-[var(--bg-canvas)] text-[var(--text-muted)]">
      <Loader2 className="h-6 w-6 animate-spin text-[var(--accent-course)]" />
      <span className="text-xs uppercase tracking-wider">Loading 1degree</span>
    </main>
  );
}

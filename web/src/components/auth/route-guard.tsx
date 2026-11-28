"use client";

import { useEffect } from "react";
import { usePathname, useRouter } from "next/navigation";
import { useAuth } from "@/lib/auth-context";
import { Loader2 } from "lucide-react";

export function RouteGuard({ children }: { children: React.ReactNode }) {
  const { user, isLoading } = useAuth();
  const router = useRouter();
  const pathname = usePathname();

  useEffect(() => {
    if (!isLoading && !user) {
      const target = pathname && pathname !== "/" ? `?next=${encodeURIComponent(pathname)}` : "";
      router.replace(`/login${target}`);
    }
  }, [isLoading, user, pathname, router]);

  useEffect(() => {
    const handlePageShow = (event: PageTransitionEvent) => {
      if (event.persisted && !user) {
        router.replace("/login");
      }
    };
    window.addEventListener("pageshow", handlePageShow);
    return () => window.removeEventListener("pageshow", handlePageShow);
  }, [user, router]);

  if (isLoading) {
    return (
      <div
        className="min-h-dvh flex flex-col items-center justify-center gap-3 bg-[var(--bg-canvas)] text-[var(--text-muted)]"
        role="status"
        aria-label="Checking session"
      >
        <Loader2 className="h-6 w-6 animate-spin text-[var(--accent-course)]" />
        <span className="text-xs tracking-wide uppercase">Loading session</span>
      </div>
    );
  }

  if (!user) {
    return null;
  }

  return <>{children}</>;
}

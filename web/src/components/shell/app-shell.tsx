"use client";

import { useEffect, useRef, useState } from "react";
import Link from "next/link";
import Image from "next/image";
import { usePathname } from "next/navigation";
import { motion, useReducedMotion } from "motion/react";
import { CalendarCheck2, Gift, LineChart, LogOut } from "lucide-react";
import { useAuth } from "@/lib/auth-context";
import { getUserInitials, isAllowedAvatarUrl } from "@/lib/auth-utils";
import { CoinIcon } from "@/components/brand/coin-icon";
import { LogoMark } from "@/components/brand/logo-mark";
import { cn } from "@/lib/utils";

const NAV_ITEMS = [
  { href: "/today", label: "Today", icon: CalendarCheck2 },
  { href: "/rewards", label: "Rewards", icon: Gift },
  { href: "/progress", label: "Progress", icon: LineChart },
] as const;

export function AppShell({ children }: { children: React.ReactNode }) {
  const { user, logout } = useAuth();
  const pathname = usePathname();
  const shouldReduceMotion = useReducedMotion();
  const [menuOpen, setMenuOpen] = useState(false);
  const menuRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!menuOpen) return;
    const onPointerDown = (e: MouseEvent) => {
      if (menuRef.current && !menuRef.current.contains(e.target as Node)) {
        setMenuOpen(false);
      }
    };
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        setMenuOpen(false);
      }
    };
    document.addEventListener("mousedown", onPointerDown);
    document.addEventListener("keydown", onKeyDown);
    return () => {
      document.removeEventListener("mousedown", onPointerDown);
      document.removeEventListener("keydown", onKeyDown);
    };
  }, [menuOpen]);

  const allowedAvatar = isAllowedAvatarUrl(user?.avatar_url);
  const initials = getUserInitials(user);
  const coinBalance = user?.coin_balance ?? 0;

  return (
    <div className="min-h-dvh w-full bg-[var(--bg-canvas)] flex justify-center">
      <div className="w-full max-w-[480px] min-h-dvh flex flex-col bg-[var(--bg-canvas)] border-x border-[var(--border-subtle)]/40 relative">
        {/* Top Bar */}
        <header className="sticky top-0 z-20 safe-pt bg-[var(--bg-canvas)]/95 backdrop-blur border-b border-[var(--border-subtle)] px-4">
          <div className="h-16 flex items-center justify-between gap-3">
            <Link
              href="/today"
              aria-label="1degree home"
              className="min-h-[44px] inline-flex items-center gap-2.5 rounded-xl pr-2 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-emerald-400"
            >
              <LogoMark size={36} />
              <span className="font-bold tracking-tight text-base text-[var(--text-primary)]">
                1degree
              </span>
            </Link>

            <div className="flex items-center gap-2">
              <div
                aria-label={`Coin balance: ${coinBalance}`}
                className="min-h-[44px] inline-flex items-center gap-2 px-3 rounded-full bg-[var(--bg-surface)] border border-amber-500/30 shadow-[0_0_16px_rgba(245,185,66,0.12)]"
              >
                <CoinIcon size={20} />
                <motion.span
                  key={coinBalance}
                  initial={shouldReduceMotion ? false : { scale: 1.18, opacity: 0.7 }}
                  animate={{ scale: 1, opacity: 1 }}
                  transition={shouldReduceMotion ? { duration: 0 } : { duration: 0.25 }}
                  className="font-game text-sm text-[var(--coin-gold)] tabular-nums"
                  data-testid="topbar-coin-balance"
                >
                  {coinBalance}
                </motion.span>
              </div>

              <div className="relative" ref={menuRef}>
                <button
                  type="button"
                  onClick={() => setMenuOpen((prev) => !prev)}
                  aria-label="Account menu"
                  aria-expanded={menuOpen}
                  aria-haspopup="menu"
                  className="min-h-[44px] min-w-[44px] inline-flex items-center justify-center rounded-full border border-[var(--border-subtle)] bg-[var(--bg-surface)] hover:border-emerald-400/50 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-emerald-400 transition overflow-hidden"
                >
                  {allowedAvatar && user?.avatar_url ? (
                    <Image
                      src={user.avatar_url}
                      alt={user.full_name || "Profile avatar"}
                      width={40}
                      height={40}
                      className="h-10 w-10 rounded-full object-cover"
                    />
                  ) : (
                    <span
                      className="h-10 w-10 rounded-full bg-[var(--bg-elevated)] text-xs font-bold text-[var(--text-primary)] flex items-center justify-center"
                      data-testid="avatar-initials"
                    >
                      {initials}
                    </span>
                  )}
                </button>

                {menuOpen && (
                  <div
                    role="menu"
                    className="absolute right-0 mt-2 w-60 rounded-2xl bg-[var(--bg-surface)] border border-[var(--border-subtle)] shadow-2xl p-2 z-30"
                  >
                    <div className="px-3 py-2 border-b border-[var(--border-subtle)] mb-1">
                      <p
                        className="text-sm font-semibold text-[var(--text-primary)] truncate"
                        data-testid="menu-user-name"
                      >
                        {user?.full_name || "1degree Explorer"}
                      </p>
                      {user?.email && (
                        <p className="text-xs text-[var(--text-muted)] truncate">
                          {user.email}
                        </p>
                      )}
                    </div>
                    <button
                      type="button"
                      role="menuitem"
                      onClick={() => {
                        setMenuOpen(false);
                        void logout();
                      }}
                      className="min-h-[44px] w-full flex items-center gap-2.5 px-3 rounded-xl text-sm font-medium text-red-300 hover:bg-red-500/10 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-red-400 transition"
                    >
                      <LogOut className="h-4 w-4" />
                      <span>Log out</span>
                    </button>
                  </div>
                )}
              </div>
            </div>
          </div>
        </header>

        {/* Main Content */}
        <main className="flex-1 px-4 pt-5 pb-28">{children}</main>

        {/* Bottom Tab Bar */}
        <nav
          aria-label="Primary navigation"
          className="fixed bottom-0 left-1/2 -translate-x-1/2 w-full max-w-[480px] z-20 safe-pb bg-[var(--bg-surface)]/95 backdrop-blur border-t border-[var(--border-subtle)]"
        >
          <div className="grid grid-cols-3 gap-1 px-2 py-1.5">
            {NAV_ITEMS.map(({ href, label, icon: Icon }) => {
              const active = pathname === href || pathname?.startsWith(`${href}/`);
              return (
                <Link
                  key={href}
                  href={href}
                  aria-label={label}
                  aria-current={active ? "page" : undefined}
                  className={cn(
                    "min-h-[44px] flex flex-col items-center justify-center gap-1 rounded-xl text-xs font-medium transition focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-emerald-400",
                    active
                      ? "bg-emerald-500/12 text-[var(--accent-course)]"
                      : "text-[var(--text-muted)] hover:text-[var(--text-primary)] hover:bg-[var(--bg-elevated)]/60"
                  )}
                >
                  <Icon className="h-4 w-4" />
                  <span>{label}</span>
                </Link>
              );
            })}
          </div>
        </nav>
      </div>
    </div>
  );
}

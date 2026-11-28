export function isEmailAuthEnabled(): boolean {
  return process.env.NEXT_PUBLIC_ENABLE_EMAIL_AUTH === "true";
}

export function sanitizeNextPath(raw: string | null | undefined): string {
  if (!raw) return "/today";
  const trimmed = raw.trim();
  if (!trimmed.startsWith("/") || trimmed.startsWith("//") || trimmed.startsWith("/\\")) {
    return "/today";
  }
  try {
    const parsed = new URL(trimmed, "http://localhost");
    if (parsed.origin !== "http://localhost") {
      return "/today";
    }
    return `${parsed.pathname}${parsed.search}${parsed.hash}`;
  } catch {
    return "/today";
  }
}

export function isAllowedAvatarUrl(url: string | null | undefined): url is string {
  if (!url) return false;
  try {
    const parsed = new URL(url.trim());
    if (parsed.protocol !== "https:") return false;
    const host = parsed.hostname.toLowerCase();
    return host === "lh3.googleusercontent.com" || host.endsWith(".googleusercontent.com");
  } catch {
    return false;
  }
}

export function getUserInitials(user: {
  full_name?: string | null;
  email?: string | null;
} | null): string {
  if (!user) return "1D";
  const name = user.full_name?.trim();
  if (name) {
    const parts = name.split(/\s+/).filter(Boolean);
    if (parts.length >= 2) {
      return `${parts[0][0]}${parts[1][0]}`.toUpperCase();
    }
    return parts[0].slice(0, 2).toUpperCase();
  }
  const email = user.email?.trim();
  if (email) {
    return email.slice(0, 2).toUpperCase();
  }
  return "1D";
}

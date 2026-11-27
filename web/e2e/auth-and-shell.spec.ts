import { test, expect, Page, BrowserContext } from "@playwright/test";
import fs from "node:fs";
import crypto from "node:crypto";

const OUTBOX_PATH = process.env.MAIL_OUTBOX_PATH || "/tmp/onedegree-mail-outbox.jsonl";
const ALLOWED_ORIGINS = new Set([
  "http://localhost:3000",
  "http://localhost:8080",
  "https://accounts.google.com",
  "https://www.gstatic.com",
  "https://ssl.gstatic.com",
  "https://fonts.gstatic.com",
]);

interface OutboxEntry {
  timestamp: string;
  type: "verification" | "password_reset";
  to: string;
  subject: string;
  token: string;
  url: string;
}

function makeSyntheticEmail(prefix: string): string {
  const rand = crypto.randomBytes(5).toString("hex");
  return `qa-${prefix}-${rand}@example.test`;
}

function makeRandomPassword(): string {
  return `QaPass!${crypto.randomBytes(8).toString("hex")}9`;
}

function readLatestOutboxEntry(
  email: string,
  type: "verification" | "password_reset",
  afterIndex = 0
): { entry: OutboxEntry; index: number } | null {
  if (!fs.existsSync(OUTBOX_PATH)) return null;
  const lines = fs
    .readFileSync(OUTBOX_PATH, "utf8")
    .split("\n")
    .map((l) => l.trim())
    .filter(Boolean);
  for (let i = lines.length - 1; i >= afterIndex; i--) {
    try {
      const parsed = JSON.parse(lines[i]) as OutboxEntry;
      if (parsed.to.toLowerCase() === email.toLowerCase() && parsed.type === type) {
        return { entry: parsed, index: i + 1 };
      }
    } catch {
      // ignore malformed line
    }
  }
  return null;
}

function outboxLineCount(): number {
  if (!fs.existsSync(OUTBOX_PATH)) return 0;
  return fs
    .readFileSync(OUTBOX_PATH, "utf8")
    .split("\n")
    .filter((l) => l.trim().length > 0).length;
}

async function waitForOutboxEntry(
  email: string,
  type: "verification" | "password_reset",
  afterIndex = 0,
  timeoutMs = 10_000
): Promise<{ entry: OutboxEntry; index: number }> {
  const start = Date.now();
  while (Date.now() - start < timeoutMs) {
    const found = readLatestOutboxEntry(email, type, afterIndex);
    if (found) return found;
    await new Promise((r) => setTimeout(r, 200));
  }
  throw new Error(`Timed out waiting for ${type} email in outbox for ${email}`);
}

async function attachHostGuard(context: BrowserContext): Promise<{
  observedHosts: Set<string>;
  blockedUrls: string[];
}> {
  const observedHosts = new Set<string>();
  const blockedUrls: string[] = [];

  await context.route("**/*", async (route) => {
    const url = route.request().url();
    if (url.startsWith("data:") || url.startsWith("blob:")) {
      await route.continue();
      return;
    }
    try {
      const parsed = new URL(url);
      observedHosts.add(parsed.origin);
      if (ALLOWED_ORIGINS.has(parsed.origin)) {
        await route.continue();
      } else {
        blockedUrls.push(url);
        await route.abort();
      }
    } catch {
      blockedUrls.push(url);
      await route.abort();
    }
  });

  return { observedHosts, blockedUrls };
}

async function assertNoTokensInWebStorage(page: Page) {
  const storageState = await page.evaluate(() => {
    return {
      cookie: document.cookie,
      localStorageKeys: Object.keys(window.localStorage),
      localStorageDump: JSON.stringify(window.localStorage),
      sessionStorageKeys: Object.keys(window.sessionStorage),
      sessionStorageDump: JSON.stringify(window.sessionStorage),
    };
  });
  expect(storageState.cookie).not.toContain("refresh_token");
  expect(storageState.cookie).not.toContain("eyJ");
  expect(storageState.localStorageKeys).toEqual([]);
  expect(storageState.sessionStorageKeys).toEqual([]);
  expect(storageState.localStorageDump).toBe("{}");
  expect(storageState.sessionStorageDump).toBe("{}");
}

test.describe("1degree Auth & Mobile-First Shell", () => {
  test("login renders at 390px & desktop, captures Google Sign-In diagnostics, and blocks protected routes without flash", async ({
    browser,
  }) => {
    const context = await browser.newContext({
      viewport: { width: 390, height: 844 },
    });
    const { observedHosts, blockedUrls } = await attachHostGuard(context);
    const page = await context.newPage();

    const consoleErrors: string[] = [];
    const pageErrors: string[] = [];
    page.on("console", (msg) => {
      if (msg.type() === "error" || msg.text().includes("GSI_LOGGER") || msg.text().includes("origin")) {
        consoleErrors.push(`[${msg.type()}] ${msg.text()}`);
      }
    });
    page.on("pageerror", (err) => pageErrors.push(err.message));

    // Protected route while signed out: must redirect to /login with zero flash of user data
    let sawProtectedContent = false;
    await page.addInitScript(() => {
      const observer = new MutationObserver(() => {
        const text = document.body?.innerText || "";
        if (text.includes("Coin Balance") || text.includes("Welcome,")) {
          (window as unknown as { __sawProtectedFlash?: boolean }).__sawProtectedFlash = true;
        }
      });
      if (document.documentElement) {
        observer.observe(document.documentElement, { childList: true, subtree: true });
      }
    });

    await page.goto("/today", { waitUntil: "domcontentloaded" });
    await page.waitForURL(/\/login/);
    sawProtectedContent = await page.evaluate(
      () => Boolean((window as unknown as { __sawProtectedFlash?: boolean }).__sawProtectedFlash)
    );
    expect(sawProtectedContent).toBe(false);

    await page.goto("/login", { waitUntil: "networkidle" });
    await expect(page.getByText("Stay 1° on course")).toBeVisible();
    await expect(page.getByTestId("google-signin-container")).toBeVisible();

    // Wait briefly so GSI script / iframe emits any origin or console message
    await page.waitForTimeout(1500);
    await page.screenshot({ path: "/tmp/pw-login-390.png", fullPage: true });

    await page.setViewportSize({ width: 1280, height: 800 });
    await page.waitForTimeout(300);
    await page.screenshot({ path: "/tmp/pw-login-desktop.png", fullPage: true });

    console.log("GSI_OBSERVED_HOSTS:", JSON.stringify([...observedHosts].sort()));
    console.log("GSI_BLOCKED_URLS:", JSON.stringify(blockedUrls));
    console.log("GSI_CONSOLE_LOGS:", JSON.stringify(consoleErrors));
    expect(pageErrors).toEqual([]);

    await context.close();
  });

  test("register, unverified block + resend, verify link strips URL, login, reload persistence, tabs, no token storage, logout + Back button", async ({
    browser,
  }) => {
    const context = await browser.newContext({
      viewport: { width: 390, height: 844 },
    });
    await attachHostGuard(context);
    const page = await context.newPage();

    const email = makeSyntheticEmail("flow");
    const password = makeRandomPassword();
    const startIdx = outboxLineCount();

    // 1. Register on /register
    await page.goto("/register");
    await page.getByLabel("Name (optional)").fill("QA Runner");
    await page.getByLabel("Email").fill(email);
    await page.getByLabel("Password (min 8 characters)").fill(password);
    await page.getByRole("button", { name: "Create account" }).click();

    // 2. See /check-email with 60s resend cooldown button
    await page.waitForURL(/\/check-email/);
    await expect(page.getByRole("heading", { name: "Check your email" })).toBeVisible();
    await expect(page.getByRole("button", { name: /Resend available in \d+s/ })).toBeDisabled();

    const { entry: verMail } = await waitForOutboxEntry(email, "verification", startIdx);
    expect(verMail.url).toMatch(/^http:\/\/localhost:3000\/verify-email\?token=/);

    // 3. Attempting login while unverified shows "Verify your email" + Resend button
    await page.goto("/login");
    await page.getByRole("tab", { name: "Email" }).click();
    await page.getByLabel("Email").fill(email);
    await page.getByLabel("Password").fill(password);
    await page.getByRole("button", { name: "Sign in" }).click();

    await expect(page.getByTestId("verify-email-notice")).toBeVisible();
    await expect(page.getByText("Verify your email")).toBeVisible();
    await expect(page.getByRole("button", { name: "Resend verification" })).toBeVisible();

    // 4. Open verification URL -> token removed from address bar -> verified state
    await page.goto(verMail.url);
    await expect(page.getByTestId("verify-success-heading")).toBeVisible();
    expect(page.url()).not.toContain("token=");

    // 5. Sign in cleanly -> lands in /today shell
    await page.getByRole("link", { name: "Continue to sign in" }).click();
    await page.waitForURL(/\/login/);
    await page.getByRole("tab", { name: "Email" }).click();
    await page.getByLabel("Email").fill(email);
    await page.getByLabel("Password").fill(password);
    await page.getByRole("button", { name: "Sign in" }).click();

    await page.waitForURL(/\/today/);
    await expect(page.getByTestId("today-greeting")).toContainText("Welcome, QA Runner");
    await expect(page.getByTestId("today-coin-balance")).toHaveText("0");
    await expect(page.getByTestId("topbar-coin-balance")).toHaveText("0");
    await page.screenshot({ path: "/tmp/pw-shell-390.png", fullPage: true });

    // Check desktop width centering (~480px)
    await page.setViewportSize({ width: 1280, height: 800 });
    await page.waitForTimeout(200);
    const navWidth = await page.getByRole("navigation", { name: "Primary navigation" }).evaluate((el) => {
      return el.getBoundingClientRect().width;
    });
    expect(navWidth).toBeLessThanOrEqual(482);
    await page.screenshot({ path: "/tmp/pw-shell-desktop.png", fullPage: true });
    await page.setViewportSize({ width: 390, height: 844 });

    // 6. Reload stays signed in & no tokens in cookie/storage
    await page.reload();
    await expect(page.getByTestId("today-greeting")).toContainText("Welcome, QA Runner");
    await assertNoTokensInWebStorage(page);

    // 7. Navigate Rewards & Progress tabs
    await page.getByRole("link", { name: "Rewards" }).click();
    await page.waitForURL(/\/rewards/);
    await expect(page.getByText("No rewards configured yet")).toBeVisible();

    await page.getByRole("link", { name: "Progress" }).click();
    await page.waitForURL(/\/progress/);
    await expect(page.getByText("No habit history yet")).toBeVisible();

    // 8. Logout via avatar menu -> Back button shows no user data
    await page.getByRole("button", { name: "Account menu" }).click();
    await expect(page.getByTestId("menu-user-name")).toHaveText("QA Runner");
    await page.getByRole("menuitem", { name: "Log out" }).click();
    await page.waitForURL(/\/login/);

    await page.goBack();
    await page.waitForURL(/\/login/);
    await expect(page.getByTestId("today-greeting")).toHaveCount(0);

    await context.close();
  });

  test("wrong password matches unknown email message, and forgot/reset password revokes old session and single-use tokens", async ({
    browser,
  }) => {
    const context = await browser.newContext({
      viewport: { width: 390, height: 844 },
    });
    await attachHostGuard(context);
    const page = await context.newPage();

    const email = makeSyntheticEmail("reset");
    const password = makeRandomPassword();
    const newPassword = makeRandomPassword();
    const startIdx = outboxLineCount();

    // Register & verify via API setup using UI
    await page.goto("/register");
    await page.getByLabel("Email").fill(email);
    await page.getByLabel("Password (min 8 characters)").fill(password);
    await page.getByRole("button", { name: "Create account" }).click();
    await page.waitForURL(/\/check-email/);
    const { entry: verMail } = await waitForOutboxEntry(email, "verification", startIdx);
    await page.goto(verMail.url);
    await expect(page.getByTestId("verify-success-heading")).toBeVisible();

    // Generic error parity: wrong password vs unknown email
    await page.goto("/login");
    await page.getByRole("tab", { name: "Email" }).click();
    await page.getByLabel("Email").fill(email);
    await page.getByLabel("Password").fill("DefinitelyWrong99!");
    await page.getByRole("button", { name: "Sign in" }).click();
    const wrongPwMsg = await page.getByTestId("login-error").innerText();

    await page.getByLabel("Email").fill(makeSyntheticEmail("missing"));
    await page.getByLabel("Password").fill("DefinitelyWrong99!");
    await page.getByRole("button", { name: "Sign in" }).click();
    const unknownMsg = await page.getByTestId("login-error").innerText();
    expect(wrongPwMsg).toBe(unknownMsg);
    expect(wrongPwMsg).toContain("Invalid email or password");

    // Sign in to establish an active session before resetting password
    await page.getByLabel("Email").fill(email);
    await page.getByLabel("Password").fill(password);
    await page.getByRole("button", { name: "Sign in" }).click();
    await page.waitForURL(/\/today/);

    // Request password reset
    const resetIdx = outboxLineCount();
    await page.goto("/forgot-password");
    await page.getByLabel("Email").fill(email);
    await page.getByRole("button", { name: "Send reset link" }).click();
    await expect(page.getByTestId("forgot-confirmation")).toBeVisible();

    const { entry: resetMail } = await waitForOutboxEntry(email, "password_reset", resetIdx);
    expect(resetMail.url).toMatch(/^http:\/\/localhost:3000\/reset-password\?token=/);

    // Open reset page -> URL immediately stripped of token -> set new password
    await page.goto(resetMail.url);
    expect(page.url()).not.toContain("token=");
    await page.getByLabel("New password (min 8 characters)").fill(newPassword);
    await page.getByLabel("Confirm new password").fill(newPassword);
    await page.getByRole("button", { name: "Update password" }).click();
    await expect(page.getByTestId("reset-success")).toBeVisible();

    // Old session must be revoked -> navigating to /today redirects to /login
    await page.goto("/today");
    await page.waitForURL(/\/login/);

    // Reusing the reset link fails cleanly
    await page.goto(resetMail.url);
    await page.getByLabel("New password (min 8 characters)").fill(makeRandomPassword());
    await page.getByLabel("Confirm new password").fill(newPassword);
    await page.getByLabel("Confirm new password").fill(await page.getByLabel("New password (min 8 characters)").inputValue());
    await page.getByRole("button", { name: "Update password" }).click();
    await expect(page.getByTestId("reset-error")).toBeVisible();

    // Invalid/expired token fails cleanly
    await page.goto("http://localhost:3000/reset-password?token=expired_or_invalid_token");
    expect(page.url()).not.toContain("token=");
    await page.getByLabel("New password (min 8 characters)").fill(newPassword);
    await page.getByLabel("Confirm new password").fill(newPassword);
    await page.getByRole("button", { name: "Update password" }).click();
    await expect(page.getByTestId("reset-error")).toBeVisible();

    // New password logs in cleanly
    await page.goto("/login");
    await page.getByRole("tab", { name: "Email" }).click();
    await page.getByLabel("Email").fill(email);
    await page.getByLabel("Password").fill(newPassword);
    await page.getByRole("button", { name: "Sign in" }).click();
    await page.waitForURL(/\/today/);

    await context.close();
  });

  test("Breaker: mocked XSS full_name & foreign avatar_url renders safely without alert or foreign image request", async ({
    browser,
  }) => {
    const context = await browser.newContext({
      viewport: { width: 390, height: 844 },
    });
    const { blockedUrls } = await attachHostGuard(context);
    const page = await context.newPage();

    let dialogFired = false;
    page.on("dialog", async (dialog) => {
      dialogFired = true;
      await dialog.dismiss();
    });

    const xssName = "<img src=x onerror=alert(1)>";
    await page.route("**/v1/auth/refresh", async (route) => {
      await route.fulfill({
        status: 200,
        contentType: "application/json",
        body: JSON.stringify({ access_token: "mock-mem-token", expires_in: 900 }),
      });
    });
    await page.route("**/v1/me", async (route) => {
      await route.fulfill({
        status: 200,
        contentType: "application/json",
        body: JSON.stringify({
          id: "00000000-0000-4000-8000-000000000001",
          email: "xss@example.test",
          full_name: xssName,
          avatar_url: "https://evil.example/avatar.png",
          timezone: "UTC",
          coin_balance: 42,
          role: "user",
          email_verified: true,
          has_premium: false,
        }),
      });
    });

    await page.goto("/today");
    await expect(page.getByTestId("today-greeting")).toContainText(`Welcome, ${xssName}`);
    await expect(page.getByTestId("avatar-initials")).toBeVisible();
    await expect(page.locator("img[src*='evil.example']")).toHaveCount(0);
    expect(dialogFired).toBe(false);
    expect(blockedUrls.some((u) => u.includes("evil.example"))).toBe(false);

    await page.getByRole("button", { name: "Account menu" }).click();
    await expect(page.getByTestId("menu-user-name")).toHaveText(xssName);
    await page.screenshot({ path: "/tmp/pw-xss-check.png", fullPage: true });

    await context.close();
  });

  test("rapid failed logins hit rate limit and UI shows friendly message", async ({
    browser,
  }) => {
    const context = await browser.newContext({
      viewport: { width: 390, height: 844 },
    });
    await attachHostGuard(context);
    const page = await context.newPage();

    await page.goto("/login");
    await page.getByRole("tab", { name: "Email" }).click();
    await page.getByLabel("Email").fill(makeSyntheticEmail("ratelimit"));
    await page.getByLabel("Password").fill("WrongPassword1!");

    let sawRateLimitMessage = false;
    for (let i = 0; i < 12; i++) {
      await page.getByRole("button", { name: "Sign in" }).click();
      const errText = await page.getByTestId("login-error").innerText();
      if (errText.toLowerCase().includes("too many requests")) {
        sawRateLimitMessage = true;
        break;
      }
    }
    expect(sawRateLimitMessage).toBe(true);
    await page.screenshot({ path: "/tmp/pw-ratelimit.png", fullPage: true });

    await context.close();
  });
});

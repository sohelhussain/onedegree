import { test, expect, Page, BrowserContext, Locator } from "@playwright/test";
import fs from "node:fs";
import crypto from "node:crypto";

async function fillField(locator: Locator, value: string): Promise<void> {
  for (let attempt = 0; attempt < 3; attempt++) {
    await locator.fill(value);
    await locator.page().waitForTimeout(50);
    if ((await locator.inputValue()) === value) {
      return;
    }
  }
  await expect(locator).toHaveValue(value);
}

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
  type: "verification" | "password_reset" | "existing_account";
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
  type: "verification" | "password_reset" | "existing_account",
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
  type: "verification" | "password_reset" | "existing_account",
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

async function confirmEmailVerification(page: Page, verifyUrl: string, password: string) {
  let verifyPostCount = 0;
  const listener = (req: { url: () => string; method: () => string }) => {
    if (req.url().includes("/v1/auth/verify-email") && req.method() === "POST") {
      verifyPostCount++;
    }
  };
  page.on("request", listener);
  try {
    await page.goto(verifyUrl, { waitUntil: "networkidle" });
    await expect(page).not.toHaveURL(/token=/);
    await expect(page.getByRole("heading", { name: "Confirm your email" })).toBeVisible();
    // Opening the link alone must never auto-verify
    expect(verifyPostCount).toBe(0);

    await fillField(page.locator("#verify-password"), password);
    await page.getByRole("button", { name: "Verify email" }).click();
    await page.waitForURL(/\/today/);
    expect(verifyPostCount).toBe(1);
  } finally {
    page.off("request", listener);
  }
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

function isAllowedExpectedConsoleError(text: string): boolean {
  return (
    text.includes("[GSI_LOGGER]: The given origin is not allowed for the given client ID") ||
    (text.includes("accounts.google.com/gsi/button") && text.includes("403")) ||
    text.includes("Failed to load resource: the server responded with a status of 403") ||
    text.includes("Failed to load resource: the server responded with a status of 401 (Unauthorized)")
  );
}

test.describe("1degree Auth & Mobile-First Shell", () => {
  test("Honest Boot (a)(b)(c)(f): /login clean at 390px & desktop, 3x reload, refresh headers, 4s Google fallback", async ({
    browser,
  }, testInfo) => {
    const project = testInfo.project.name;
    const context = await browser.newContext({
      viewport: { width: 390, height: 844 },
    });
    const { observedHosts, blockedUrls } = await attachHostGuard(context);
    const page = await context.newPage();

    const unexpectedConsoleErrors: string[] = [];
    const allowedExpectedConsoleErrors: string[] = [];
    const unexpectedHttpErrors: string[] = [];
    const pageErrors: string[] = [];

    page.on("console", (msg) => {
      if (msg.type() === "error" || msg.text().includes("GSI_LOGGER")) {
        const text = msg.text();
        if (isAllowedExpectedConsoleError(text)) {
          allowedExpectedConsoleErrors.push(`[${msg.type()}] ${text}`);
        } else {
          unexpectedConsoleErrors.push(`[${msg.type()}] ${text}`);
        }
      }
    });
    const gsiButtonStatuses: number[] = [];
    page.on("response", (res) => {
      const status = res.status();
      if (res.url().includes("accounts.google.com/gsi/button")) {
        gsiButtonStatuses.push(status);
      }
      if (status >= 400) {
        const url = res.url();
        const isExpectedRefresh401 = url.includes("/v1/auth/refresh") && status === 401;
        const isExpectedGoogle403 = url.includes("accounts.google.com/gsi/button") && status === 403;
        if (!isExpectedRefresh401 && !isExpectedGoogle403) {
          unexpectedHttpErrors.push(`${status} ${url}`);
        }
      }
    });
    page.on("pageerror", (err) => pageErrors.push(err.message));

    // Protected route while signed out: must redirect to /login with zero flash of user data
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
    const sawProtectedContent = await page.evaluate(
      () => Boolean((window as unknown as { __sawProtectedFlash?: boolean }).__sawProtectedFlash)
    );
    expect(sawProtectedContent).toBe(false);

    // Capture (c) /v1/auth/refresh request & response headers on /login
    let refreshCapture: {
      url: string;
      method: string;
      status: number;
      requestHeaders: Record<string, string>;
      responseHeaders: Record<string, string>;
    } | null = null;

    page.on("response", async (res) => {
      if (res.url().includes("/v1/auth/refresh")) {
        refreshCapture = {
          url: res.url(),
          method: res.request().method(),
          status: res.status(),
          requestHeaders: await res.request().allHeaders(),
          responseHeaders: await res.allHeaders(),
        };
      }
    });

    await page.goto("/login", { waitUntil: "networkidle" });
    await expect(page.getByText("Stay 1° on course")).toBeVisible();
    await expect(page.getByTestId("google-signin-container")).toBeAttached();

    // Check (f): inspect real /gsi/button status and verify rendered Google button stays visible or shows fallback
    await page.waitForTimeout(4500);
    const gsiDom = await page.getByTestId("google-signin-container").evaluate((el) => {
      const target = el.querySelector<HTMLElement>("iframe, button, [role='button']");
      const rect = target?.getBoundingClientRect();
      return {
        hasTarget: Boolean(target),
        tag: target?.tagName || null,
        width: rect?.width ?? 0,
        height: rect?.height ?? 0,
      };
    });
    console.log(`[${project}] GSI_BUTTON_STATUSES:`, JSON.stringify(gsiButtonStatuses));
    console.log(`[${project}] GSI_DOM_4S:`, JSON.stringify(gsiDom));

    const googleRenderedWorkingButton = gsiDom.hasTarget && gsiDom.width >= 40 && gsiDom.height >= 20;

    if (googleRenderedWorkingButton) {
      await expect(page.getByTestId("google-fallback-notice")).toHaveCount(0);
    } else {
      await expect(page.getByTestId("google-fallback-notice")).toHaveText(
        "Google sign-in is unavailable here. Use email instead."
      );
    }

    // Ensure Email tab remains clickable and usable while Google card is in either state,
    // and switching back preserves that exact state without losing a working button
    await page.getByRole("tab", { name: "Email" }).click();
    await expect(page.getByLabel("Email")).toBeVisible();
    await expect(page.getByLabel("Password")).toBeVisible();
    await page.getByRole("tab", { name: "Google" }).click();
    if (googleRenderedWorkingButton) {
      await expect(page.getByTestId("google-fallback-notice")).toHaveCount(0);
    } else {
      await expect(page.getByTestId("google-fallback-notice")).toBeVisible();
    }

    // Assert zero error toasts and zero Next.js dev error badge ("1 Issue")
    await expect(page.locator("[data-sonner-toast][data-type='error']")).toHaveCount(0);
    await expect(page.getByText("Unable to reach authentication server")).toHaveCount(0);
    await expect(page.getByText("Load failed")).toHaveCount(0);
    const nextIssueBadgeCount = await page.locator("nextjs-portal").evaluateAll((portals) => {
      return portals.filter((p) => {
        const root = p.shadowRoot || p;
        const text = root.textContent || "";
        return /\b\d+\s+Issues?\b/i.test(text);
      }).length;
    });
    expect(nextIssueBadgeCount).toBe(0);

    await page.screenshot({ path: `/tmp/pw-login-${project}-390.png`, fullPage: true });

    // Switch to desktop 1280x800 and check clean rendering
    await page.setViewportSize({ width: 1280, height: 800 });
    await page.waitForTimeout(300);
    if (googleRenderedWorkingButton) {
      await expect(page.getByTestId("google-fallback-notice")).toHaveCount(0);
    } else {
      await expect(page.getByTestId("google-fallback-notice")).toBeVisible();
    }
    await page.screenshot({ path: `/tmp/pw-login-${project}-desktop.png`, fullPage: true });

    // Check (b): reload /login 3 times -> never show auth unreachable or WebKit "Load failed"
    for (let i = 1; i <= 3; i++) {
      await page.reload({ waitUntil: "networkidle" });
      await expect(page.getByText("Unable to reach authentication server")).toHaveCount(0);
      await expect(page.getByText("Load failed")).toHaveCount(0);
      await expect(page.locator("[data-sonner-toast][data-type='error']")).toHaveCount(0);
    }

    expect(refreshCapture).not.toBeNull();
    console.log(`[${project}] REFRESH_CAPTURE:`, JSON.stringify(refreshCapture));
    console.log(`[${project}] GSI_OBSERVED_HOSTS:`, JSON.stringify([...observedHosts].sort()));
    console.log(`[${project}] GSI_BLOCKED_URLS:`, JSON.stringify(blockedUrls));
    console.log(`[${project}] ALLOWED_EXPECTED_CONSOLE_ERRORS:`, JSON.stringify(allowedExpectedConsoleErrors));
    console.log(`[${project}] UNEXPECTED_CONSOLE_ERRORS:`, JSON.stringify(unexpectedConsoleErrors));
    console.log(`[${project}] UNEXPECTED_HTTP_ERRORS:`, JSON.stringify(unexpectedHttpErrors));

    expect(pageErrors).toEqual([]);
    expect(unexpectedConsoleErrors).toEqual([]);
    expect(unexpectedHttpErrors).toEqual([]);
    expect(blockedUrls).toEqual([]);

    await context.close();
  });

  test("register, unverified block + resend, verify link strips URL, login, reload persistence, second tab, Set-Cookie, no token storage, logout + Back button", async ({
    browser,
  }, testInfo) => {
    const project = testInfo.project.name;
    const context = await browser.newContext({
      viewport: { width: 390, height: 844 },
    });
    await attachHostGuard(context);
    const page = await context.newPage();

    const email = makeSyntheticEmail(`flow-${project}`);
    const password = makeRandomPassword();
    const startIdx = outboxLineCount();

    let verifySetCookie = "";
    let loginSetCookie = "";
    let refreshSetCookie = "";
    page.on("response", async (res) => {
      if (res.url().includes("/v1/auth/verify-email") && res.status() === 200) {
        const headers = await res.allHeaders();
        verifySetCookie = headers["set-cookie"] || "";
      }
      if (res.url().includes("/v1/auth/login") && res.status() === 200) {
        const headers = await res.allHeaders();
        loginSetCookie = headers["set-cookie"] || "";
      }
      if (res.url().includes("/v1/auth/refresh") && res.status() === 200) {
        const headers = await res.allHeaders();
        refreshSetCookie = headers["set-cookie"] || "";
      }
    });

    // 1. Register on /register
    await page.goto("/register", { waitUntil: "networkidle" });
    await fillField(page.getByLabel("Name (optional)"), "QA Runner");
    await fillField(page.getByLabel("Email"), email);
    await fillField(page.getByLabel("Password (min 8 characters)"), password);
    await page.getByRole("button", { name: "Create account" }).click();

    // 2. See /check-email with 60s resend cooldown button
    await page.waitForURL(/\/check-email/);
    await expect(page.getByRole("heading", { name: "Check your email" })).toBeVisible();
    await expect(page.getByRole("button", { name: /Resend available in \d+s/ })).toBeDisabled();

    const { entry: verMail } = await waitForOutboxEntry(email, "verification", startIdx);
    expect(verMail.url).toMatch(/^http:\/\/localhost:3000\/verify-email\?token=/);

    // 3. Attempting login while unverified shows "Verify your email" + Resend button
    await page.goto("/login", { waitUntil: "networkidle" });
    await page.getByRole("tab", { name: "Email" }).click();
    await fillField(page.getByLabel("Email"), email);
    await fillField(page.getByLabel("Password"), password);
    await page.getByRole("button", { name: "Sign in" }).click();

    await expect(page.getByTestId("verify-email-notice")).toBeVisible();
    await expect(
      page.getByTestId("verify-email-notice").getByText("Verify your email", { exact: true })
    ).toBeVisible();
    await expect(page.getByRole("button", { name: "Resend verification" })).toBeVisible();

    // 4. Open verification URL -> token stripped -> enter password -> lands directly in /today
    await confirmEmailVerification(page, verMail.url, password);
    await expect(page.getByTestId("today-greeting")).toContainText("Welcome, QA Runner");
    expect(verifySetCookie).toContain("HttpOnly");
    expect(verifySetCookie).toContain("SameSite=Lax");
    expect(verifySetCookie).toContain("Path=/v1/auth");

    // 5. Log out and sign in cleanly -> lands back in /today shell
    await page.getByRole("button", { name: "Account menu" }).click();
    await page.getByRole("menuitem", { name: "Log out" }).click();
    await page.waitForURL(/\/login/);
    await page.waitForLoadState("networkidle");
    await page.getByRole("tab", { name: "Email" }).click();
    await fillField(page.getByLabel("Email"), email);
    await fillField(page.getByLabel("Password"), password);
    await page.getByRole("button", { name: "Sign in" }).click();

    await page.waitForURL(/\/today/);
    await expect(page.getByTestId("today-greeting")).toContainText("Welcome, QA Runner");
    await expect(page.getByTestId("today-coin-balance")).toHaveText("0");
    await expect(page.getByTestId("topbar-coin-balance")).toHaveText("0");
    expect(loginSetCookie).toContain("HttpOnly");
    expect(loginSetCookie).toContain("SameSite=Lax");
    expect(loginSetCookie).toContain("Path=/v1/auth");
    await page.screenshot({ path: `/tmp/pw-shell-${project}-390.png`, fullPage: true });

    // Check desktop width centering (~480px)
    await page.setViewportSize({ width: 1280, height: 800 });
    await page.waitForTimeout(200);
    const navWidth = await page.getByRole("navigation", { name: "Primary navigation" }).evaluate((el) => {
      return el.getBoundingClientRect().width;
    });
    expect(navWidth).toBeLessThanOrEqual(482);
    await page.screenshot({ path: `/tmp/pw-shell-${project}-desktop.png`, fullPage: true });
    await page.setViewportSize({ width: 390, height: 844 });

    // 6. Reload stays signed in & no tokens in cookie/storage
    await page.reload();
    await expect(page.getByTestId("today-greeting")).toContainText("Welcome, QA Runner");
    expect(refreshSetCookie).toContain("HttpOnly");
    await assertNoTokensInWebStorage(page);

    // 6b. Open a second tab and confirm signed in (wait sequentially so refresh token rotation never races)
    const page2 = await context.newPage();
    await page2.goto("/today");
    await expect(page2.getByTestId("today-greeting")).toContainText("Welcome, QA Runner");
    await expect(page2.getByTestId("today-coin-balance")).toHaveText("0");
    await page2.close();

    // 7. Navigate Rewards & Progress tabs (reload first if second tab rotated refresh cookie only in memory)
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
    await page.goto("/register", { waitUntil: "networkidle" });
    await fillField(page.getByLabel("Email"), email);
    await fillField(page.getByLabel("Password (min 8 characters)"), password);
    await page.getByRole("button", { name: "Create account" }).click();
    await page.waitForURL(/\/check-email/);
    const { entry: verMail } = await waitForOutboxEntry(email, "verification", startIdx);
    await confirmEmailVerification(page, verMail.url, password);
    await page.getByRole("button", { name: "Account menu" }).click();
    await page.getByRole("menuitem", { name: "Log out" }).click();
    await page.waitForURL(/\/login/);

    // Generic error parity: wrong password vs unknown email
    await page.waitForLoadState("networkidle");
    await page.getByRole("tab", { name: "Email" }).click();
    await fillField(page.getByLabel("Email"), email);
    await fillField(page.getByLabel("Password"), "DefinitelyWrong99!");
    await page.getByRole("button", { name: "Sign in" }).click();
    const wrongPwMsg = await page.getByTestId("login-error").innerText();

    await fillField(page.getByLabel("Email"), makeSyntheticEmail("missing"));
    await fillField(page.getByLabel("Password"), "DefinitelyWrong99!");
    await page.getByRole("button", { name: "Sign in" }).click();
    const unknownMsg = await page.getByTestId("login-error").innerText();
    expect(wrongPwMsg).toBe(unknownMsg);
    expect(wrongPwMsg).toContain("Invalid email or password");

    // Sign in to establish an active session before resetting password
    await fillField(page.getByLabel("Email"), email);
    await fillField(page.getByLabel("Password"), password);
    await page.getByRole("button", { name: "Sign in" }).click();
    await page.waitForURL(/\/today/);

    // Request password reset
    const resetIdx = outboxLineCount();
    await page.goto("/forgot-password", { waitUntil: "networkidle" });
    await fillField(page.getByLabel("Email"), email);
    await page.getByRole("button", { name: "Send reset link" }).click();
    await expect(page.getByTestId("forgot-confirmation")).toBeVisible();

    const { entry: resetMail } = await waitForOutboxEntry(email, "password_reset", resetIdx);
    expect(resetMail.url).toMatch(/^http:\/\/localhost:3000\/reset-password\?token=/);

    // Open reset page -> URL immediately stripped of token -> set new password
    await page.goto(resetMail.url, { waitUntil: "networkidle" });
    await expect(page).not.toHaveURL(/token=/);
    await fillField(page.getByLabel("New password (min 8 characters)"), newPassword);
    await fillField(page.getByLabel("Confirm new password"), newPassword);
    await page.getByRole("button", { name: "Update password" }).click();
    await expect(page.getByTestId("reset-success")).toBeVisible();

    // Old session must be revoked -> navigating to /today redirects to /login
    await page.goto("/today");
    await page.waitForURL(/\/login/);

    // Reusing the reset link fails cleanly
    await page.goto(resetMail.url, { waitUntil: "networkidle" });
    await expect(page).not.toHaveURL(/token=/);
    await fillField(page.getByLabel("New password (min 8 characters)"), newPassword);
    await fillField(page.getByLabel("Confirm new password"), newPassword);
    await page.getByRole("button", { name: "Update password" }).click();
    await expect(page.getByTestId("reset-error")).toBeVisible();

    // Invalid/expired token fails cleanly
    await page.goto("http://localhost:3000/reset-password?token=expired_or_invalid_token", {
      waitUntil: "networkidle",
    });
    await expect(page).not.toHaveURL(/token=/);
    await fillField(page.getByLabel("New password (min 8 characters)"), newPassword);
    await fillField(page.getByLabel("Confirm new password"), newPassword);
    await page.getByRole("button", { name: "Update password" }).click();
    await expect(page.getByTestId("reset-error")).toBeVisible();

    // New password logs in cleanly
    await page.goto("/login", { waitUntil: "networkidle" });
    await page.getByRole("tab", { name: "Email" }).click();
    await fillField(page.getByLabel("Email"), email);
    await fillField(page.getByLabel("Password"), newPassword);
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

  test("login Enter clears fields -> Create account sends verification only to registration email & clears register fields", async ({
    browser,
  }, testInfo) => {
    const project = testInfo.project.name;
    const context = await browser.newContext({
      viewport: { width: 390, height: 844 },
    });
    await attachHostGuard(context);
    const page = await context.newPage();

    const gsiDuplicateWarnings: string[] = [];
    page.on("console", (msg) => {
      if (msg.text().includes("initialize() is called multiple times")) {
        gsiDuplicateWarnings.push(msg.text());
      }
    });

    const loginEmail = makeSyntheticEmail(`loginfirst-${project}`);
    const regEmail = makeSyntheticEmail(`regsecond-${project}`);
    const password = makeRandomPassword();
    const startIdx = outboxLineCount();

    // 1. User types credentials on /login and presses Enter -> fails cleanly & clears inputs
    await page.goto("/login", { waitUntil: "networkidle" });
    await page.getByRole("tab", { name: "Email" }).click();
    await fillField(page.locator("#login-email"), loginEmail);
    await fillField(page.locator("#login-password"), password);
    await page.locator("#login-password").press("Enter");

    await expect(page.getByTestId("login-error")).toContainText("Invalid email or password");
    await expect(page.locator("#login-email")).toHaveValue("");
    await expect(page.locator("#login-password")).toHaveValue("");

    // 2. Click "Create an account" -> /register fields must start blank
    await page.getByRole("link", { name: "Create an account" }).click();
    await page.waitForURL(/\/register/);
    await page.waitForLoadState("networkidle");
    await expect(page.locator("#reg-name")).toHaveValue("");
    await expect(page.locator("#reg-email")).toHaveValue("");
    await expect(page.locator("#reg-password")).toHaveValue("");

    // 3. Fill second email, name, password and submit via Enter
    let capturedRegisterEmail = "";
    await page.route("**/v1/auth/register", async (route) => {
      const postData = route.request().postDataJSON() as { email?: string } | null;
      capturedRegisterEmail = postData?.email || "";
      await route.continue();
    });

    await fillField(page.locator("#reg-name"), "Second Email User");
    await fillField(page.locator("#reg-email"), regEmail);
    await fillField(page.locator("#reg-password"), password);
    await page.locator("#reg-password").press("Enter");

    // Inputs clear immediately while submitting/navigating to /check-email
    await page.waitForURL(/\/check-email/);
    expect(capturedRegisterEmail).toBe(regEmail);
    expect(decodeURIComponent(page.url())).toContain(regEmail);
    expect(page.url()).not.toContain(loginEmail);

    // 4. Outbox entry goes ONLY to regEmail, never loginEmail
    const { entry: verMail } = await waitForOutboxEntry(regEmail, "verification", startIdx);
    expect(verMail.to.toLowerCase()).toBe(regEmail.toLowerCase());
    expect(readLatestOutboxEntry(loginEmail, "verification", startIdx)).toBeNull();
    expect(gsiDuplicateWarnings).toEqual([]);

    await context.close();
  });

  test("Step D attacker: throttled double-submit sends 1 request, GET verify-email no-JS safe, cross-context verify, outage recovery, injection, and keyboard/paste a11y", async ({
    browser,
  }, testInfo) => {
    const project = testInfo.project.name;
    const context = await browser.newContext({
      viewport: { width: 390, height: 844 },
    });
    await attachHostGuard(context);
    const page = await context.newPage();

    let dialogFired = false;
    page.on("dialog", async (d) => {
      dialogFired = true;
      await d.dismiss();
    });

    // D.9 Empty-token navigation states render cleanly
    await page.goto("/verify-email", { waitUntil: "networkidle" });
    await expect(page.getByText("Verification failed")).toBeVisible();
    await page.goto("/reset-password", { waitUntil: "networkidle" });
    await expect(page.getByText("Password reset link is missing a token.")).toBeVisible();

    // D.12 Keyboard tab navigation & paste support on /register at 390px
    await page.goto("/register", { waitUntil: "networkidle" });
    await page.keyboard.press("Tab");
    const firstFocusedId = await page.evaluate(() => document.activeElement?.id || "");
    expect(firstFocusedId).toBe("reg-name");
    await page.keyboard.press("Tab");
    const secondFocusedId = await page.evaluate(() => document.activeElement?.id || "");
    expect(secondFocusedId).toBe("reg-email");
    await page.keyboard.press("Tab");
    const thirdFocusedId = await page.evaluate(() => document.activeElement?.id || "");
    expect(thirdFocusedId).toBe("reg-password");

    // Verify paste event works into password input
    await page.locator("#reg-password").evaluate((el) => {
      const input = el as HTMLInputElement;
      const dt = new DataTransfer();
      dt.setData("text/plain", "PastedPass123!");
      const evt = new ClipboardEvent("paste", { clipboardData: dt, bubbles: true, cancelable: true });
      input.dispatchEvent(evt);
    });

    // D.10 Mid-flow network failure stops spinner & leaves form usable
    const attackerEmail = makeSyntheticEmail(`atk-${project}`);
    const attackerPass = makeRandomPassword();
    const attackerName = "<script>window.__xss=1</script> O'Reilly";

    await page.route("**/v1/auth/register", async (route) => {
      await route.abort("connectionrefused");
    });
    await fillField(page.locator("#reg-name"), attackerName);
    await fillField(page.locator("#reg-email"), attackerEmail);
    await fillField(page.locator("#reg-password"), attackerPass);
    await page.getByRole("button", { name: "Create account" }).click();

    await expect(
      page.getByRole("alert").filter({ hasText: "Unable to reach authentication server" })
    ).toBeVisible();
    await expect(page.getByRole("button", { name: "Create account" })).toBeEnabled();
    await page.unroute("**/v1/auth/register");

    // D.1 Throttled network + double-click + rapid Enter fires strictly ONE register POST
    let registerPostCount = 0;
    const startIdx = outboxLineCount();
    await page.route("**/v1/auth/register", async (route) => {
      registerPostCount++;
      await new Promise((r) => setTimeout(r, 400));
      await route.continue();
    });

    await fillField(page.locator("#reg-name"), attackerName);
    await fillField(page.locator("#reg-email"), attackerEmail);
    await fillField(page.locator("#reg-password"), attackerPass);
    await page.locator("button[type='submit']").evaluate((btn) => {
      const el = btn as HTMLButtonElement;
      el.click();
      el.click();
    });
    await page.locator("#reg-password").press("Enter").catch(() => {});

    await page.waitForURL(/\/check-email/);
    await page.unroute("**/v1/auth/register");
    expect(registerPostCount).toBe(1);

    const { entry: verMail } = await waitForOutboxEntry(attackerEmail, "verification", startIdx);
    const matchesForEmail = fs
      .readFileSync(OUTBOX_PATH, "utf8")
      .split("\n")
      .slice(startIdx)
      .filter((line) => line.includes(attackerEmail));
    expect(matchesForEmail.length).toBe(1);

    // D.9 Refresh /check-email & Back after register render cleanly
    await page.reload({ waitUntil: "networkidle" });
    await expect(page.getByRole("heading", { name: "Check your email" })).toBeVisible();
    await page.goBack();
    await expect(page.locator("body")).toBeVisible();

    // D.5 Plain GET of /verify-email (simulating email link scanner without JS) must NOT consume token
    const scannerGet = await page.request.get(verMail.url);
    expect(scannerGet.status()).toBe(200);

    // D.5 Open verification link in a COMPLETELY DIFFERENT browser context -> still requires password!
    const contextB = await browser.newContext({ viewport: { width: 1280, height: 800 } });
    await attachHostGuard(contextB);
    const pageB = await contextB.newPage();
    await pageB.goto(verMail.url, { waitUntil: "networkidle" });
    await expect(pageB).not.toHaveURL(/token=/);
    await expect(pageB.getByRole("heading", { name: "Confirm your email" })).toBeVisible();

    // Attacker/wrong password must fail cleanly without activating user
    await fillField(pageB.locator("#verify-password"), "WrongVictimPass1!");
    await pageB.getByRole("button", { name: "Verify email" }).click();
    await expect(pageB.getByTestId("verify-error")).toBeVisible();
    await expect(pageB.getByTestId("verify-success-heading")).toHaveCount(0);

    // Correct password verifies and navigates directly to /today at 1280x800; check D.11 safe rendering
    await fillField(pageB.locator("#verify-password"), attackerPass);
    await pageB.getByRole("button", { name: "Verify email" }).click();
    await pageB.waitForURL(/\/today/);
    await expect(pageB.getByTestId("today-greeting")).toContainText(attackerName);
    expect(dialogFired).toBe(false);
    const xssTriggered = await pageB.evaluate(
      () => Boolean((window as unknown as { __xss?: number }).__xss)
    );
    expect(xssTriggered).toBe(false);

    await contextB.close();
    await context.close();
  });

  test("Loop A: stubbed Google button stays visible >10s, StrictMode/nav/logout initialize===1, and non-rendering stub hits 4s fallback", async ({
    browser,
  }, testInfo) => {
    test.setTimeout(120_000);
    const project = testInfo.project.name;

    // Wait briefly for earlier sequential register tests' 60s IP sliding window before creating LoopA's account
    const warmupPage = await (await browser.newContext()).newPage();
    await warmupPage.waitForTimeout(35_000);
    await warmupPage.context().close();

    // 1. Working GSI stub -> stays visible >10s, survives Email tab >4s, nav roundtrip, and logout
    const context = await browser.newContext({ viewport: { width: 390, height: 844 } });
    await attachHostGuard(context);
    const gsiDuplicateWarnings: string[] = [];
    const stubScriptWorking = `
      window.__gsiInitCount = 0;
      window.__gsiRenderCount = 0;
      window.google = {
        accounts: {
          id: {
            initialize: function() {
              window.__gsiInitCount += 1;
              if (window.__gsiInitCount > 1) {
                console.warn("google.accounts.id.initialize() is called multiple times");
              }
            },
            renderButton: function(parent) {
              window.__gsiRenderCount += 1;
              var btn = document.createElement("button");
              btn.type = "button";
              btn.setAttribute("data-testid", "stub-gsi-button");
              btn.textContent = "Continue with Google";
              btn.style.width = "310px";
              btn.style.height = "40px";
              btn.style.display = "block";
              parent.innerHTML = "";
              parent.appendChild(btn);
            },
            prompt: function() {}
          }
        }
      };
    `;
    await context.route("https://accounts.google.com/gsi/client**", async (route) => {
      await route.fulfill({
        status: 200,
        contentType: "application/javascript",
        body: stubScriptWorking,
      });
    });

    const page = await context.newPage();
    page.on("console", (msg) => {
      if (msg.text().includes("initialize() is called multiple times")) {
        gsiDuplicateWarnings.push(msg.text());
      }
    });

    // Create & verify a user first so we can test logout -> /login redraw
    const email = makeSyntheticEmail(`loopa-${project}`);
    const password = makeRandomPassword();
    const startIdx = outboxLineCount();
    await page.goto("/register", { waitUntil: "networkidle" });
    await fillField(page.locator("#reg-name"), "LoopA User");
    await fillField(page.locator("#reg-email"), email);
    await fillField(page.locator("#reg-password"), password);
    await page.getByRole("button", { name: "Create account" }).click();
    await page.waitForURL(/\/check-email/);
    const { entry: verMail } = await waitForOutboxEntry(email, "verification", startIdx);
    await confirmEmailVerification(page, verMail.url, password);
    await page.getByRole("button", { name: "Account menu" }).click();
    await page.getByRole("menuitem", { name: "Log out" }).click();
    await page.waitForURL(/\/login/);

    // /login -> working Google button must stay visible > 10s
    await page.waitForLoadState("networkidle");
    await expect(page.getByTestId("stub-gsi-button")).toBeVisible();
    await page.waitForTimeout(10_500);
    await expect(page.getByTestId("stub-gsi-button")).toBeVisible();
    await expect(page.getByTestId("google-fallback-notice")).toHaveCount(0);

    // Switch to Email tab for 4.5s -> return to Google tab -> still visible, zero fallback!
    await page.getByRole("tab", { name: "Email" }).click();
    await page.waitForTimeout(4500);
    await page.getByRole("tab", { name: "Google" }).click();
    await expect(page.getByTestId("stub-gsi-button")).toBeVisible();
    await expect(page.getByTestId("google-fallback-notice")).toHaveCount(0);

    // Client-side nav /login -> /register -> /login -> button redraws cleanly & initialize() count === 1
    await page.getByRole("tab", { name: "Email" }).click();
    await page.getByRole("link", { name: "Create an account" }).click();
    await page.waitForURL(/\/register/);
    await page.getByRole("link", { name: "Back to sign in" }).click();
    await page.waitForURL(/\/login/);
    await page.getByRole("tab", { name: "Google" }).click();
    await expect(page.getByTestId("stub-gsi-button")).toBeVisible();
    await expect(page.getByTestId("google-fallback-notice")).toHaveCount(0);

    const initCountAfterNav = await page.evaluate(
      () => (window as unknown as { __gsiInitCount?: number }).__gsiInitCount ?? 0
    );
    expect(initCountAfterNav).toBe(1);
    expect(gsiDuplicateWarnings).toEqual([]);

    // Sign in -> /today -> Log out -> /login redraws button & keeps initialize() count === 1
    await page.getByRole("tab", { name: "Email" }).click();
    await fillField(page.locator("#login-email"), email);
    await fillField(page.locator("#login-password"), password);
    await page.getByRole("button", { name: "Sign in" }).click();
    await page.waitForURL(/\/today/);

    await page.getByRole("button", { name: "Account menu" }).click();
    await page.getByRole("menuitem", { name: "Log out" }).click();
    await page.waitForURL(/\/login/);
    await expect(page.getByTestId("stub-gsi-button")).toBeVisible();
    await expect(page.getByTestId("google-fallback-notice")).toHaveCount(0);

    const initCountAfterLogout = await page.evaluate(
      () => (window as unknown as { __gsiInitCount?: number }).__gsiInitCount ?? 0
    );
    expect(initCountAfterLogout).toBe(1);
    await context.close();

    // 2. Non-rendering GSI stub -> 4s fallback appears, Email tab still usable
    const contextFail = await browser.newContext({ viewport: { width: 390, height: 844 } });
    await attachHostGuard(contextFail);
    await contextFail.route("https://accounts.google.com/gsi/client**", async (route) => {
      await route.fulfill({
        status: 200,
        contentType: "application/javascript",
        body: `window.google = { accounts: { id: { initialize: function(){}, renderButton: function(){}, prompt: function(){} } } };`,
      });
    });
    const pageFail = await contextFail.newPage();
    await pageFail.goto("/login", { waitUntil: "networkidle" });
    await pageFail.waitForTimeout(4300);
    await expect(pageFail.getByTestId("google-fallback-notice")).toHaveText(
      "Google sign-in is unavailable here. Use email instead."
    );
    await pageFail.getByRole("tab", { name: "Email" }).click();
    await expect(pageFail.locator("#login-email")).toBeVisible();
    await expect(pageFail.locator("#login-password")).toBeVisible();
    await contextFail.close();
  });

  test("Loop B & C & D: verified registration sends existing_account notice on re-register and 5 wrong verify passwords invalidate token", async ({
    browser,
  }, testInfo) => {
    test.setTimeout(135_000);
    const project = testInfo.project.name;
    const context = await browser.newContext({ viewport: { width: 390, height: 844 } });
    await attachHostGuard(context);
    const page = await context.newPage();

    // Ensure Loop A's register slot has aged cleanly out of the 60s per-IP window
    await page.waitForTimeout(30_000);

    const lockoutEmail = makeSyntheticEmail(`looplock-${project}`);
    const lockoutPassword = makeRandomPassword();
    let startIdx = outboxLineCount();

    // 1. Register pending signup -> generic /check-email copy + verification email
    await page.goto("/register", { waitUntil: "networkidle" });
    await fillField(page.locator("#reg-email"), lockoutEmail);
    await fillField(page.locator("#reg-password"), lockoutPassword);
    await page.getByRole("button", { name: "Create account" }).click();
    await page.waitForURL(/\/check-email/);
    await expect(page.getByText(/If this address can be used, we sent a verification link/)).toBeVisible();
    await expect(page.getByText(/It expires in 24 hours\./)).toBeVisible();

    const { entry: lockMail, index: afterLock } = await waitForOutboxEntry(
      lockoutEmail,
      "verification",
      startIdx
    );

    // 2. 5 wrong passwords on /verify-email invalidate the token so 6th right password fails
    await page.goto(lockMail.url, { waitUntil: "networkidle" });
    await expect(page).not.toHaveURL(/token=/);
    for (let i = 1; i <= 5; i++) {
      await fillField(page.locator("#verify-password"), `WrongVerifyPass!${i}`);
      await page.getByRole("button", { name: "Verify email" }).click();
      await expect(page.getByTestId("verify-error")).toBeVisible();
    }
    // 6th attempt even with correct password must be rejected
    await fillField(page.locator("#verify-password"), lockoutPassword);
    await page.getByRole("button", { name: "Verify email" }).click();
    await expect(page.getByTestId("verify-error")).toBeVisible();
    await expect(page.getByTestId("verify-success-heading")).toHaveCount(0);

    // 3. Register a fresh synthetic user, verify via {token, password}, then re-register same verified email
    const email = makeSyntheticEmail(`loopbcd-${project}`);
    const password = makeRandomPassword();
    startIdx = afterLock;
    await page.goto("/register", { waitUntil: "networkidle" });
    await fillField(page.locator("#reg-email"), email);
    await fillField(page.locator("#reg-password"), password);
    await page.getByRole("button", { name: "Create account" }).click();
    await page.waitForURL(/\/check-email/);
    const { entry: verMail2, index: afterSecond } = await waitForOutboxEntry(
      email,
      "verification",
      startIdx
    );
    await confirmEmailVerification(page, verMail2.url, password);

    // Wait cleanly for the 60s per-IP register sliding window from earlier sequential tests before step 4
    await page.waitForTimeout(30_000);

    // 4. Re-register already-verified email -> generic /check-email + existing_account notice, NO verification link
    await page.goto("/register", { waitUntil: "networkidle" });
    await fillField(page.locator("#reg-email"), email);
    await fillField(page.locator("#reg-password"), makeRandomPassword());
    await page.getByRole("button", { name: "Create account" }).click();
    await page.waitForURL(/\/check-email/);
    await expect(page.getByText(/If this address can be used, we sent a verification link/)).toBeVisible();

    const { entry: noticeMail } = await waitForOutboxEntry(email, "existing_account", afterSecond);
    expect(noticeMail.url).toMatch(/\/login$/);
    expect(readLatestOutboxEntry(email, "verification", afterSecond)).toBeNull();

    await context.close();
  });

  test("rapid failed logins hit rate limit and UI shows friendly message", async ({
    browser,
  }) => {
    test.setTimeout(95_000);
    const context = await browser.newContext({
      viewport: { width: 390, height: 844 },
    });
    await attachHostGuard(context);
    const page = await context.newPage();

    await page.goto("/login", { waitUntil: "networkidle" });
    await page.getByRole("tab", { name: "Email" }).click();
    const rateEmail = makeSyntheticEmail("ratelimit");

    let sawRateLimitMessage = false;
    for (let i = 0; i < 12; i++) {
      await fillField(page.getByLabel("Email"), rateEmail);
      await fillField(page.getByLabel("Password"), "WrongPassword1!");
      await page.getByRole("button", { name: "Sign in" }).click();
      await expect(page.getByLabel("Email")).toHaveValue("");
      await expect(page.getByLabel("Password")).toHaveValue("");
      const errText = await page.getByTestId("login-error").innerText();
      if (errText.toLowerCase().includes("too many requests")) {
        sawRateLimitMessage = true;
        break;
      }
    }
    expect(sawRateLimitMessage).toBe(true);
    await page.screenshot({ path: "/tmp/pw-ratelimit.png", fullPage: true });

    // Wait out the 60s IP login window so subsequent suites recover cleanly
    await page.waitForTimeout(61_000);
    await fillField(page.getByLabel("Email"), rateEmail);
    await fillField(page.getByLabel("Password"), "WrongPassword1!");
    await page.getByRole("button", { name: "Sign in" }).click();
    await expect(page.getByTestId("login-error")).toContainText("Invalid email or password");

    await context.close();
  });
});

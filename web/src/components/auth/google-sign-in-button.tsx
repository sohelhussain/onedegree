"use client";

import { useEffect, useRef, useState } from "react";
import Script from "next/script";
import { useAuth } from "@/lib/auth-context";
import { toast } from "sonner";

declare global {
  interface Window {
    google?: {
      accounts: {
        id: {
          initialize: (config: {
            client_id: string;
            callback: (response: { credential: string }) => void;
            auto_select?: boolean;
          }) => void;
          renderButton: (
            parent: HTMLElement,
            options: {
              theme?: "outline" | "filled_blue" | "filled_black";
              size?: "large" | "medium" | "small";
              text?: "signin_with" | "signup_with" | "continue_with";
              shape?: "rectangular" | "pill" | "circle";
              width?: number;
            }
          ) => void;
          prompt: () => void;
        };
      };
    };
  }
}

let gsiInitializedClientId: string | null = null;
let gsiCredentialHandler: ((credential: string) => void) | null = null;

function hasRenderedGsiButton(container: HTMLElement | null): boolean {
  if (!container) return false;
  const semanticTarget = container.querySelector<HTMLElement>(
    "iframe, button, [role='button']"
  );
  if (semanticTarget) {
    const rect = semanticTarget.getBoundingClientRect();
    return rect.width >= 40 && rect.height >= 20;
  }
  for (const child of Array.from(container.children)) {
    if (child instanceof HTMLElement) {
      const rect = child.getBoundingClientRect();
      if (rect.width >= 40 && rect.height >= 20) {
        return true;
      }
    }
  }
  return false;
}

export function GoogleSignInButton({
  onSuccess,
  visible = true,
}: {
  onSuccess?: () => void;
  visible?: boolean;
}) {
  const { loginWithGoogle, isLoading } = useAuth();
  const buttonRef = useRef<HTMLDivElement>(null);
  const loginRef = useRef(loginWithGoogle);
  const onSuccessRef = useRef(onSuccess);
  const renderedRef = useRef(false);
  const [scriptLoaded, setScriptLoaded] = useState(false);
  const [scriptFailed, setScriptFailed] = useState(false);
  const [fallbackVisible, setFallbackVisible] = useState(false);
  const clientId = process.env.NEXT_PUBLIC_GOOGLE_CLIENT_ID || "";
  const hasValidClientId = Boolean(clientId) && !clientId.includes("your-google-client-id");

  useEffect(() => {
    loginRef.current = loginWithGoogle;
  }, [loginWithGoogle]);

  useEffect(() => {
    onSuccessRef.current = onSuccess;
  }, [onSuccess]);

  useEffect(() => {
    if (!hasValidClientId || scriptLoaded || scriptFailed) return;
    if (window.google?.accounts?.id) {
      Promise.resolve().then(() => setScriptLoaded(true));
      return;
    }
    const pollId = window.setInterval(() => {
      if (window.google?.accounts?.id) {
        setScriptLoaded(true);
        window.clearInterval(pollId);
      }
    }, 150);
    return () => window.clearInterval(pollId);
  }, [hasValidClientId, scriptLoaded, scriptFailed]);

  useEffect(() => {
    if (!hasValidClientId || !visible || scriptFailed || fallbackVisible) return;

    let timer: number | null = window.setTimeout(() => {
      timer = null;
      if (!hasRenderedGsiButton(buttonRef.current)) {
        setFallbackVisible(true);
      }
    }, 4000);

    const cancelIfRendered = () => {
      if (timer !== null && hasRenderedGsiButton(buttonRef.current)) {
        window.clearTimeout(timer);
        timer = null;
      }
    };

    cancelIfRendered();

    const container = buttonRef.current;
    const mutationObserver =
      container && typeof MutationObserver !== "undefined"
        ? new MutationObserver(cancelIfRendered)
        : null;
    if (container && mutationObserver) {
      mutationObserver.observe(container, {
        childList: true,
        subtree: true,
        attributes: true,
      });
    }

    const resizeObserver =
      container && typeof ResizeObserver !== "undefined"
        ? new ResizeObserver(cancelIfRendered)
        : null;
    if (container && resizeObserver) {
      resizeObserver.observe(container);
    }

    return () => {
      if (timer !== null) {
        window.clearTimeout(timer);
      }
      mutationObserver?.disconnect();
      resizeObserver?.disconnect();
    };
  }, [hasValidClientId, visible, scriptFailed, fallbackVisible]);

  useEffect(() => {
    const container = buttonRef.current;
    if (!scriptLoaded || !window.google || !container || !hasValidClientId) {
      return;
    }

    gsiCredentialHandler = (credential: string) => {
      void loginRef.current(credential).then(() => {
        onSuccessRef.current?.();
      });
    };

    try {
      if (gsiInitializedClientId !== clientId) {
        window.google.accounts.id.initialize({
          client_id: clientId,
          callback: (res) => {
            if (res.credential && gsiCredentialHandler) {
              gsiCredentialHandler(res.credential);
            }
          },
        });
        gsiInitializedClientId = clientId;
      }

      if (!renderedRef.current && container.childElementCount === 0) {
        window.google.accounts.id.renderButton(container, {
          theme: "filled_black",
          size: "large",
          text: "continue_with",
          shape: "pill",
          width: 310,
        });
      }
      renderedRef.current = true;
    } catch {
      toast.error("Google sign-in failed to initialize.");
      Promise.resolve().then(() => {
        setScriptFailed(true);
        setFallbackVisible(true);
      });
    }

    return () => {
      renderedRef.current = false;
      container.innerHTML = "";
    };
  }, [scriptLoaded, clientId, hasValidClientId]);

  return (
    <div className="flex flex-col items-center gap-2 w-full">
      {hasValidClientId && (
        <Script
          src="https://accounts.google.com/gsi/client"
          strategy="afterInteractive"
          onLoad={() => setScriptLoaded(true)}
          onError={() => {
            setScriptFailed(true);
            setFallbackVisible(true);
            toast.error("Could not load Google Sign-In script.");
          }}
        />
      )}

      {!hasValidClientId && (
        <button
          type="button"
          disabled
          className="min-h-[44px] w-full max-w-[310px] px-4 rounded-full bg-[var(--bg-elevated)] border border-[var(--border-subtle)] text-sm text-[var(--text-muted)] opacity-80 cursor-not-allowed"
        >
          Google Sign-In unavailable (client ID not set)
        </button>
      )}

      <div
        ref={buttonRef}
        data-testid="google-signin-container"
        aria-busy={isLoading}
        className={
          fallbackVisible
            ? "hidden"
            : "min-h-[44px] w-full flex items-center justify-center"
        }
      />

      {hasValidClientId && (fallbackVisible || scriptFailed) && (
        <p
          data-testid="google-fallback-notice"
          className="min-h-[44px] w-full max-w-[310px] px-4 py-2.5 rounded-full bg-[var(--bg-elevated)] border border-[var(--border-subtle)] text-xs text-[var(--text-muted)] text-center flex items-center justify-center"
          role="status"
        >
          Google sign-in is unavailable here. Use email instead.
        </p>
      )}
    </div>
  );
}

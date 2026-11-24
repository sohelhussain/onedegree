"use client";

import { useEffect, useRef, useState } from "react";
import Script from "next/script";
import { useAuth } from "@/lib/auth-context";
import { KeyRound, Loader2 } from "lucide-react";

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

export function GoogleSignInButton() {
  const { loginWithGoogle, isLoading } = useAuth();
  const buttonRef = useRef<HTMLDivElement>(null);
  const [scriptLoaded, setScriptLoaded] = useState(false);
  const [showManualInput, setShowManualInput] = useState(false);
  const [manualToken, setManualToken] = useState("");
  const clientId = process.env.NEXT_PUBLIC_GOOGLE_CLIENT_ID || "";

  useEffect(() => {
    if (!scriptLoaded || !window.google || !buttonRef.current) return;

    if (!clientId || clientId.includes("your-google-client-id")) {
      return;
    }

    try {
      window.google.accounts.id.initialize({
        client_id: clientId,
        callback: (res) => {
          if (res.credential) {
            loginWithGoogle(res.credential);
          }
        },
      });

      window.google.accounts.id.renderButton(buttonRef.current, {
        theme: "filled_black",
        size: "large",
        text: "continue_with",
        shape: "pill",
        width: 280,
      });
    } catch (e) {
      console.warn("Could not initialize Google Identity Services:", e);
    }
  }, [scriptLoaded, clientId, loginWithGoogle]);

  const handleManualSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    if (manualToken.trim()) {
      loginWithGoogle(manualToken.trim());
    }
  };

  return (
    <div className="flex flex-col items-center gap-3">
      <Script
        src="https://accounts.google.com/gsi/client"
        strategy="afterInteractive"
        onLoad={() => setScriptLoaded(true)}
      />

      {/* Official Google Button Render Target */}
      <div ref={buttonRef} className="min-h-[44px] flex items-center justify-center">
        {!clientId || clientId.includes("your-google-client-id") ? (
          <button
            type="button"
            onClick={() => setShowManualInput(!showManualInput)}
            className="flex items-center gap-2 px-5 py-2.5 rounded-full bg-zinc-900 border border-zinc-700 text-zinc-200 text-sm font-medium hover:bg-zinc-800 transition"
          >
            <svg className="h-4 w-4" viewBox="0 0 24 24">
              <path
                fill="#4285F4"
                d="M22.56 12.25c0-.78-.07-1.53-.2-2.25H12v4.26h5.92c-.26 1.37-1.04 2.53-2.21 3.31v2.77h3.57c2.08-1.92 3.28-4.74 3.28-8.09z"
              />
              <path
                fill="#34A853"
                d="M12 23c2.97 0 5.46-.98 7.28-2.66l-3.57-2.77c-.98.66-2.23 1.06-3.71 1.06-2.86 0-5.29-1.93-6.16-4.53H2.18v2.84C3.99 20.53 7.7 23 12 23z"
              />
              <path
                fill="#FBBC05"
                d="M5.84 14.09c-.22-.66-.35-1.36-.35-2.09s.13-1.43.35-2.09V7.06H2.18C1.43 8.55 1 10.22 1 12s.43 3.45 1.18 4.94l2.85-2.22.81-.63z"
              />
              <path
                fill="#EA4335"
                d="M12 5.38c1.62 0 3.06.56 4.21 1.64l3.15-3.15C17.45 2.09 14.97 1 12 1 7.7 1 3.99 3.47 2.18 7.06l3.66 2.84c.87-2.6 3.3-4.52 6.16-4.52z"
              />
            </svg>
            <span>Sign in with Google</span>
          </button>
        ) : null}
      </div>

      {/* Manual / Testing ID Token Prompt */}
      <div className="w-full text-center">
        <button
          type="button"
          onClick={() => setShowManualInput(!showManualInput)}
          className="text-xs text-zinc-500 hover:text-zinc-300 underline underline-offset-4 transition"
        >
          {showManualInput ? "Hide token input" : "Test with direct ID token"}
        </button>

        {showManualInput && (
          <form onSubmit={handleManualSubmit} className="mt-3 flex flex-col gap-2 w-full max-w-sm mx-auto">
            <textarea
              value={manualToken}
              onChange={(e) => setManualToken(e.target.value)}
              placeholder="Paste Google JWT id_token here..."
              rows={3}
              className="w-full p-2 text-xs font-mono bg-zinc-900 border border-zinc-800 rounded-lg text-zinc-300 focus:outline-none focus:border-emerald-500"
            />
            <button
              type="submit"
              disabled={isLoading || !manualToken.trim()}
              className="flex items-center justify-center gap-2 px-4 py-2 rounded-lg bg-emerald-600 hover:bg-emerald-500 text-white text-xs font-semibold disabled:opacity-50 transition"
            >
              {isLoading ? (
                <Loader2 className="h-3.5 w-3.5 animate-spin" />
              ) : (
                <KeyRound className="h-3.5 w-3.5" />
              )}
              Verify & Sign In via /v1/auth/google
            </button>
          </form>
        )}
      </div>
    </div>
  );
}

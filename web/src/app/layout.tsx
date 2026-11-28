import type { Metadata, Viewport } from "next";
import { Geist_Mono, Plus_Jakarta_Sans, Rubik_Mono_One } from "next/font/google";
import { Toaster } from "sonner";
import { AuthProvider } from "@/lib/auth-context";
import "./globals.css";

const plusJakarta = Plus_Jakarta_Sans({
  variable: "--font-sans-body",
  subsets: ["latin"],
});

const geistMono = Geist_Mono({
  variable: "--font-geist-mono",
  subsets: ["latin"],
});

const rubikMono = Rubik_Mono_One({
  variable: "--font-game-display",
  weight: "400",
  subsets: ["latin"],
});

export const metadata: Metadata = {
  title: "1degree — Stay 1° on course",
  description:
    "Small, non-negotiable daily micro-actions compound into massive personal transformation over 365 days.",
};

export const viewport: Viewport = {
  width: "device-width",
  initialScale: 1,
  viewportFit: "cover",
  themeColor: "#090b10",
};

export default function RootLayout({
  children,
}: Readonly<{
  children: React.ReactNode;
}>) {
  return (
    <html
      lang="en"
      className={`${plusJakarta.variable} ${geistMono.variable} ${rubikMono.variable} h-full antialiased dark`}
    >
      <body className="min-h-dvh flex flex-col bg-[var(--bg-canvas)] text-[var(--text-primary)] selection:bg-emerald-500/30 selection:text-emerald-200">
        <AuthProvider>
          {children}
          <Toaster position="top-center" richColors theme="dark" />
        </AuthProvider>
      </body>
    </html>
  );
}

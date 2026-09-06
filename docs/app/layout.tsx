import type { Metadata } from "next";
import type { ReactNode } from "react";

import SiteHeader from "@/components/SiteHeader";

import "./globals.css";

export const metadata: Metadata = {
  title: {
    default: "Stellar Stream Hive Docs",
    template: "%s | Stellar Stream Hive Docs",
  },
  description:
    "Documentation for Stellar Stream Hive: real-time, on-chain payment streaming built on Stellar and Soroban.",
};

export default function RootLayout({ children }: { children: ReactNode }) {
  return (
    <html lang="en">
      <body className="min-h-screen bg-white text-slate-900 antialiased dark:bg-slate-950 dark:text-slate-100">
        <SiteHeader />
        {children}
      </body>
    </html>
  );
}

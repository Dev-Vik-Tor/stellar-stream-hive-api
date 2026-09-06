import type { ReactNode } from "react";

import Sidebar from "@/components/Sidebar";

export default function DocsLayout({ children }: { children: ReactNode }) {
  return (
    <div className="mx-auto flex max-w-7xl flex-col lg:flex-row">
      <Sidebar />
      <main className="min-w-0 flex-1 px-6 py-10 lg:px-12">
        <article className="prose prose-slate mx-auto max-w-3xl dark:prose-invert prose-headings:scroll-mt-20 prose-a:text-brand-600 dark:prose-a:text-brand-400">
          {children}
        </article>
      </main>
    </div>
  );
}

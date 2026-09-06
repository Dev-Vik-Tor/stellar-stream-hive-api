"use client";

import Link from "next/link";
import { usePathname } from "next/navigation";

import { nav } from "@/lib/nav";

export default function Sidebar() {
  const pathname = usePathname();

  return (
    <nav
      aria-label="Documentation navigation"
      className="w-full shrink-0 border-slate-200 px-4 py-8 dark:border-slate-800 lg:sticky lg:top-16 lg:h-[calc(100vh-4rem)] lg:w-64 lg:overflow-y-auto lg:border-r"
    >
      {nav.map((group) => (
        <div key={group.title} className="mb-6">
          <p className="mb-2 px-2 text-xs font-semibold uppercase tracking-wider text-slate-400 dark:text-slate-500">
            {group.title}
          </p>
          <ul className="space-y-1">
            {group.items.map((item) => {
              const active = pathname === item.href;
              return (
                <li key={item.href}>
                  <Link
                    href={item.href}
                    aria-current={active ? "page" : undefined}
                    className={`block rounded-md px-2 py-1.5 text-sm transition ${
                      active
                        ? "bg-brand-100 font-medium text-brand-800 dark:bg-brand-900/40 dark:text-brand-200"
                        : "text-slate-600 hover:bg-slate-100 hover:text-slate-900 dark:text-slate-400 dark:hover:bg-slate-800 dark:hover:text-slate-100"
                    }`}
                  >
                    {item.title}
                  </Link>
                </li>
              );
            })}
          </ul>
        </div>
      ))}
    </nav>
  );
}

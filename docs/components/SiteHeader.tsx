import Link from "next/link";

const REPOS = [
  { label: "API", href: "https://github.com/Stellar-Hive/stellar-stream-hive-api" },
  { label: "Contracts", href: "https://github.com/Stellar-Hive/stellar-stream-hive-contract" },
  { label: "Web App", href: "https://github.com/Stellar-Hive/stellar-stream-hive-frontend" },
];

export default function SiteHeader() {
  return (
    <header className="sticky top-0 z-40 flex h-16 items-center justify-between border-b border-slate-200 bg-white/80 px-6 backdrop-blur dark:border-slate-800 dark:bg-slate-950/80">
      <Link href="/" className="flex items-center gap-2 font-semibold text-slate-900 dark:text-slate-100">
        <span aria-hidden className="text-xl">
          🌊
        </span>
        Stellar Stream Hive
      </Link>
      <nav className="flex items-center gap-5 text-sm">
        <Link
          href="/docs/getting-started"
          className="text-slate-600 hover:text-slate-900 dark:text-slate-400 dark:hover:text-slate-100"
        >
          Docs
        </Link>
        {REPOS.map((repo) => (
          <a
            key={repo.href}
            href={repo.href}
            target="_blank"
            rel="noreferrer"
            className="hidden text-slate-600 hover:text-slate-900 sm:inline dark:text-slate-400 dark:hover:text-slate-100"
          >
            {repo.label}
          </a>
        ))}
      </nav>
    </header>
  );
}

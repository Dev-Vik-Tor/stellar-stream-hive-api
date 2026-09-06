import type { ReactNode } from "react";

type Variant = "info" | "warning" | "success" | "danger";

const STYLES: Record<Variant, { wrapper: string; icon: string; label: string }> = {
  info: {
    wrapper:
      "border-blue-400 bg-blue-50 text-blue-900 dark:border-blue-800 dark:bg-blue-950 dark:text-blue-100",
    icon: "ℹ️",
    label: "Note",
  },
  warning: {
    wrapper:
      "border-amber-400 bg-amber-50 text-amber-900 dark:border-amber-800 dark:bg-amber-950 dark:text-amber-100",
    icon: "⚠️",
    label: "Warning",
  },
  success: {
    wrapper:
      "border-emerald-400 bg-emerald-50 text-emerald-900 dark:border-emerald-800 dark:bg-emerald-950 dark:text-emerald-100",
    icon: "✅",
    label: "Tip",
  },
  danger: {
    wrapper:
      "border-red-400 bg-red-50 text-red-900 dark:border-red-800 dark:bg-red-950 dark:text-red-100",
    icon: "🚨",
    label: "Security",
  },
};

/** A titled, colored callout used throughout the docs for notes, warnings, and security call-outs. */
export default function CalloutBox({
  variant = "info",
  title,
  children,
}: {
  variant?: Variant;
  title?: string;
  children: ReactNode;
}) {
  const style = STYLES[variant];
  return (
    <div className={`my-6 rounded-lg border-l-4 px-4 py-3 ${style.wrapper}`} role="note">
      <p className="mb-1 flex items-center gap-2 font-semibold">
        <span aria-hidden>{style.icon}</span>
        {title ?? style.label}
      </p>
      <div className="text-sm leading-relaxed [&>p]:m-0 [&>p+p]:mt-2">{children}</div>
    </div>
  );
}

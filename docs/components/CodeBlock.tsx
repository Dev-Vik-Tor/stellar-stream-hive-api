"use client";

import { isValidElement, useState, type ReactElement, type ReactNode } from "react";

/** Recursively flattens MDX-rendered code children back into plain text for the copy button. */
function extractText(node: ReactNode): string {
  if (typeof node === "string" || typeof node === "number") return String(node);
  if (Array.isArray(node)) return node.map(extractText).join("");
  if (isValidElement(node)) {
    const props = node.props as { children?: ReactNode };
    return extractText(props.children);
  }
  return "";
}

/**
 * Replaces MDX's default `<pre>` rendering for fenced code blocks. MDX passes the
 * fenced block's language as a `language-xxx` class on the inner `<code>` element
 * (e.g. ` ```rust ` produces `<code class="language-rust">`), which we surface as
 * a small label, plus a copy-to-clipboard button.
 */
export default function CodeBlock(props: { children?: ReactNode }) {
  const [copied, setCopied] = useState(false);
  const { children, ...rest } = props;

  const codeElement = isValidElement(children) ? (children as ReactElement<{ className?: string; children?: ReactNode }>) : null;
  const className = codeElement?.props.className ?? "";
  const match = /language-(\w+)/.exec(className);
  const language = match?.[1] ?? "text";
  const text = extractText(children).replace(/\n$/, "");

  const handleCopy = async () => {
    try {
      await navigator.clipboard.writeText(text);
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1500);
    } catch {
      // Clipboard API unavailable (e.g. non-secure context) -- the code block
      // remains fully selectable by hand, so this is a silent no-op.
    }
  };

  return (
    <div className="group relative my-6 overflow-hidden rounded-lg border border-slate-800 bg-slate-950">
      <div className="flex items-center justify-between border-b border-slate-800 bg-slate-900 px-4 py-1.5">
        <span className="text-xs font-medium uppercase tracking-wide text-slate-400">{language}</span>
        <button
          type="button"
          onClick={handleCopy}
          className="rounded px-2 py-1 text-xs font-medium text-slate-400 transition hover:bg-slate-800 hover:text-slate-100"
        >
          {copied ? "Copied" : "Copy"}
        </button>
      </div>
      <pre className="overflow-x-auto p-4 text-sm leading-relaxed text-slate-100" {...rest}>
        {children}
      </pre>
    </div>
  );
}

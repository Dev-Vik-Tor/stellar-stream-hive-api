export interface NavItem {
  title: string;
  href: string;
}

export interface NavGroup {
  title: string;
  items: NavItem[];
}

/** Single source of truth for the docs sidebar, in reading order. */
export const nav: NavGroup[] = [
  {
    title: "Introduction",
    items: [{ title: "Getting Started", href: "/docs/getting-started" }],
  },
  {
    title: "Concepts",
    items: [
      { title: "Streaming Math", href: "/docs/concepts/streaming-math" },
      { title: "Security Model", href: "/docs/concepts/security-model" },
      { title: "Glossary", href: "/docs/concepts/glossary" },
    ],
  },
  {
    title: "Contracts",
    items: [
      { title: "Stream Contract", href: "/docs/contracts/stream" },
      { title: "Vault Contract", href: "/docs/contracts/vault" },
      { title: "Registry Contract", href: "/docs/contracts/registry" },
    ],
  },
  {
    title: "API Reference",
    items: [
      { title: "Overview", href: "/docs/api/overview" },
      { title: "Streams", href: "/docs/api/streams" },
      { title: "Stats", href: "/docs/api/stats" },
    ],
  },
  {
    title: "Guides",
    items: [
      { title: "Web App", href: "/docs/web-app" },
      { title: "Integration", href: "/docs/integration" },
    ],
  },
];

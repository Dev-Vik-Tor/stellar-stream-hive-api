# Contributing to Stellar Stream Hive API

> **Scope of this repository: contributors work in `docs/` only.**
>
> This repository hosts both the Rust indexer API (`api/`) and its
> documentation site (`docs/`). **`api/` is maintainer-only.** Pull requests
> that modify anything under `api/` will be closed without review, regardless
> of how small or well-intentioned the change is. This is a deliberate policy,
> not an oversight -- the API handles indexing logic for a payments platform,
> and changes to it go through a separate, more careful review process outside
> of general open-source contributions. If you've found a bug in the API or
> want to propose a change to it, please open an issue describing the problem
> instead of a PR with a fix.
>
> Everything below applies to `docs/` contributions.

## What we welcome in `docs/`

- Fixing typos, broken links, or outdated information.
- Clarifying explanations, especially in the [Streaming Math](docs/app/docs/concepts/streaming-math/page.mdx)
  and [Security Model](docs/app/docs/concepts/security-model/page.mdx) pages --
  these are the pages most likely to be read by someone auditing the platform,
  so precision matters more here than anywhere else in the docs.
- Adding examples (curl, TypeScript, Rust) that make an existing page more
  concrete, as long as they're accurate against the current contract/API
  behavior.
- Improving the docs site itself: components, styling, navigation, accessibility.

## What we don't want

- New pages that duplicate content covered elsewhere -- open an issue first if
  you think something is missing, so we can agree on where it belongs.
- Speculative documentation for contract behavior that doesn't exist yet.
  Contract addresses and deployment details are placeholders until the
  contracts repo actually deploys (see
  [Getting Started](docs/app/docs/getting-started/page.mdx)) -- please don't
  submit PRs that replace those placeholders with guesses.
- Any change under `api/` (see above).

## Local development

```bash
cd docs
npm install
npm run dev
```

This starts the docs site at `http://localhost:3000` with hot reload. Edit any
`page.mdx` file under `docs/app/docs/` and the browser updates automatically.

Before opening a PR, confirm the site still builds cleanly:

```bash
npm run build
```

## MDX conventions

- Every doc page is `app/docs/<section>/page.mdx` and exports a `metadata`
  object with `title` and `description`:

  ```mdx
  export const metadata = {
    title: "Your Page Title",
    description: "One sentence describing the page.",
  };
  ```

- Use the shared components instead of raw HTML/Tailwind where one exists:
  - `<CalloutBox variant="info" | "warning" | "success" | "danger">` for
    notes, warnings, tips, and security call-outs (`components/CalloutBox.tsx`).
  - `<ApiEndpoint method="GET" path="/streams/:id" description="..." />` at
    the top of an API reference section (`components/ApiEndpoint.tsx`).
  - Fenced code blocks (` ```typescript `, ` ```rust `, ` ```bash `, etc.) are
    automatically rendered through `components/CodeBlock.tsx` with a language
    label and copy button -- no need to wrap them in anything yourself.
- Internal links use root-relative paths without a trailing slash, e.g.
  `[Streaming Math](/docs/concepts/streaming-math)`. Link to a specific
  heading with `#kebab-case-of-the-heading-text`, e.g.
  `/docs/concepts/streaming-math#why-multiply-before-divide`.
- Add new pages to the sidebar by editing `docs/lib/nav.ts` -- the sidebar has
  no other configuration; a page that exists but isn't in `nav.ts` is
  reachable by direct link but won't appear in navigation.
- Numeric/technical claims (formulas, worked examples, function signatures)
  should match the actual contract/API behavior described elsewhere in the
  docs or in `api/src/`. If you're not sure something is still accurate,
  flag it in your PR description rather than guessing.

## Pull request checklist

- [ ] Changes are entirely within `docs/` (or this `CONTRIBUTING.md`/root
      `README.md`, for meta changes).
- [ ] `npm run build` succeeds in `docs/`.
- [ ] New pages are added to `docs/lib/nav.ts`.
- [ ] Internal links resolve (no typos in paths).
- [ ] No fabricated contract addresses, deployment details, or numeric claims.

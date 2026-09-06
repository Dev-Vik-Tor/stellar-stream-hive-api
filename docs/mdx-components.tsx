import type { MDXComponents } from "mdx/types";

import CodeBlock from "@/components/CodeBlock";

/**
 * Global overrides applied to every MDX doc page. `CalloutBox` and `ApiEndpoint`
 * are intentionally *not* included here -- pages import them explicitly since
 * they take props (variant, method, path) that a blanket override can't supply.
 */
export function useMDXComponents(components: MDXComponents): MDXComponents {
  return {
    pre: (props) => <CodeBlock {...props} />,
    table: (props) => (
      <div className="my-6 overflow-x-auto">
        <table {...props} />
      </div>
    ),
    ...components,
  };
}

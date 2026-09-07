import createMDX from "@next/mdx";
import rehypeSlug from "rehype-slug";

/** @type {import('next').NextConfig} */
const nextConfig = {
  pageExtensions: ["ts", "tsx", "mdx"],
  reactStrictMode: true,
};

const withMDX = createMDX({
  extension: /\.mdx?$/,
  options: {
    // Gives every heading an id derived from its text (e.g. "## Why multiply
    // before divide" -> id="why-multiply-before-divide"), which is what every
    // in-page and cross-page `#anchor` link in these docs relies on.
    rehypePlugins: [rehypeSlug],
  },
});

export default withMDX(nextConfig);

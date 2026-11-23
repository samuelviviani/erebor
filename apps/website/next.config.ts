import type { NextConfig } from "next";

// Fully static site: no server, no cookies, no analytics. Deployable on Vercel or any static host.
const config: NextConfig = {
  output: "export",
  images: { unoptimized: true },
  poweredByHeader: false,
};

export default config;

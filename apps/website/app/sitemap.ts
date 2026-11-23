import type { MetadataRoute } from "next";
import { site } from "@/site";

export const dynamic = "force-static";

const routes = ["", "/download", "/fees", "/docs", "/faq", "/open-source", "/privacy", "/terms"];

export default function sitemap(): MetadataRoute.Sitemap {
  return routes.map((r) => ({ url: `${site.url}${r}`, changeFrequency: "monthly", priority: r === "" ? 1 : 0.6 }));
}

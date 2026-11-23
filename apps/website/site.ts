import releases from "./releases.json";

export const site = {
  name: "Erebor",
  tagline: "Mining, under your control.",
  description:
    "Erebor è un mining manager open source e non-custodial: gestisci il mining dal tuo computer, monitora prestazioni, consumi e ricompense, mantenendo il controllo del tuo wallet.",
  url: process.env.NEXT_PUBLIC_SITE_URL || "http://localhost:3000",
  repoUrl: process.env.NEXT_PUBLIC_REPO_URL || null,
  devFeePercent: 2,
  xmrigDonatePercent: 1,
};

export interface ReleaseAsset {
  platform: string;
  label: string;
  file: string;
  url: string;
  sha256: string;
  size: number;
}

/** Filled by the release workflow. Only platforms with a real, published build appear. */
export const release = releases as { version: string | null; date: string | null; assets: ReleaseAsset[] };

export const nav = [
  { href: "/#come-funziona", label: "Come funziona" },
  { href: "/#funzionalita", label: "Funzionalità" },
  { href: "/fees", label: "Commissioni" },
  { href: "/docs", label: "Documentazione" },
  { href: "/faq", label: "FAQ" },
];

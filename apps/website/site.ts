export const site = {
  name: "Erebor",
  tagline: "Mining, without the maze.",
  description:
    "Erebor is a source-available, local-first mining manager for your own computer: it picks the coin, runs verified miners and shows what it really earns.",
  url: process.env.NEXT_PUBLIC_SITE_URL || "http://localhost:3000",
  repoUrl: "https://github.com/samuelviviani/erebor",
  devFeePercent: 2,
  xmrigDonatePercent: 1,
};

export const latestRelease = `${site.repoUrl}/releases/latest`;
export const installCommand = `curl -fsSL https://raw.githubusercontent.com/samuelviviani/erebor/main/scripts/install.sh | sh`;

export const nav = [
  { href: "/#how-it-works", label: "How it works" },
  { href: "/#features", label: "Features" },
  { href: "/fees", label: "Fees" },
  { href: "/docs", label: "Docs" },
  { href: "/faq", label: "FAQ" },
];

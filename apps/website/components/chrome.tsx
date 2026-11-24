import Link from "next/link";
import type { ReactNode } from "react";
import { nav, site } from "@/site";

export function Logo({ className }: { className?: string }) {
  return (
    // eslint-disable-next-line @next/next/no-img-element
    <img src="/logo-mark.svg" alt="" aria-hidden className={className} />
  );
}

export function Header() {
  return (
    <header className="sticky top-0 z-20 border-b border-line bg-bg/90 backdrop-blur">
      <div className="mx-auto flex h-16 max-w-6xl items-center justify-between gap-4 px-4 sm:px-6">
        <Link href="/" className="flex items-center gap-2.5 font-semibold tracking-wide">
          <Logo className="h-7 w-auto" />
          Erebor
        </Link>
        <nav aria-label="Main" className="hidden items-center gap-6 text-sm text-muted md:flex">
          {nav.map((n) => (
            <Link key={n.href} href={n.href} className="hover:text-fg">
              {n.label}
            </Link>
          ))}
        </nav>
        <Link
          href="/download"
          className="inline-flex h-9 items-center rounded-lg bg-accent-solid px-4 text-sm font-medium text-white hover:brightness-110"
        >
          Download
        </Link>
      </div>
    </header>
  );
}

export function Footer() {
  return (
    <footer className="mt-24 border-t border-line">
      <div className="mx-auto grid max-w-6xl gap-8 px-4 py-10 text-sm text-muted sm:grid-cols-3 sm:px-6">
        <div>
          <div className="mb-2 flex items-center gap-2 font-semibold text-fg">
            <Logo className="h-5 w-auto" /> Erebor
          </div>
          <p className="text-faint">Open-source, non-custodial, local-first mining manager.</p>
        </div>
        <ul className="space-y-2">
          <li><Link href="/download" className="hover:text-fg">Download</Link></li>
          <li><Link href="/docs" className="hover:text-fg">Docs</Link></li>
          <li><Link href="/fees" className="hover:text-fg">Fees</Link></li>
          <li><Link href="/open-source" className="hover:text-fg">Open source</Link></li>
        </ul>
        <ul className="space-y-2">
          <li><Link href="/faq" className="hover:text-fg">FAQ</Link></li>
          <li><Link href="/privacy" className="hover:text-fg">Privacy</Link></li>
          <li><Link href="/terms" className="hover:text-fg">Terms of use</Link></li>
          <li><a href={site.repoUrl} className="hover:text-fg" rel="noopener">GitHub</a></li>
        </ul>
      </div>
      <p className="mx-auto max-w-6xl px-4 pb-10 text-xs text-faint sm:px-6">
        Erebor is not affiliated with any miner or pool. Mining costs electricity and guarantees no return.
      </p>
    </footer>
  );
}

export function Doc({ title, lead, children }: { title: string; lead?: string; children: ReactNode }) {
  return (
    <article className="prose-erebor mx-auto max-w-3xl px-4 pt-16 sm:px-6">
      <h1 className="mb-3 text-3xl font-semibold tracking-tight text-fg">{title}</h1>
      {lead && <p className="text-lg">{lead}</p>}
      {children}
    </article>
  );
}

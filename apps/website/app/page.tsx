import Link from "next/link";
import { latestRelease, site } from "@/site";

const steps = [
  { t: "Install Erebor", d: "Download the app for Windows, macOS or Linux and check the file's hash. No account needed." },
  { t: "Tell it where earnings go", d: "Paste your Kryptex ID: earnings arrive in Bitcoin on your Kryptex account. Erebor never asks for seed phrases or private keys." },
  { t: "Your PC gets ready", d: "Erebor recognises your hardware, downloads the official miners and verifies them, with no technical questions." },
  { t: "Start when you want", d: "Mining starts only when you press Start, and stops when you say so or when you quit." },
];

const features = [
  { t: "It picks the coin", d: "It reads public profitability data and mines what pays most on your hardware today, re-checking every half hour. You can always choose yourself." },
  { t: "Verified miners only", d: "Official releases of XMRig, SRBMiner-MULTI, lolMiner and cpuminer-opt, checked against a SHA-256 pinned in the source code." },
  { t: "Real earnings", d: "Estimated earnings minus electricity, next to a link to your real balance on Kryptex. Missing data is shown as missing, never guessed." },
  { t: "Hardware safety", d: "Eco, Balanced and Performance modes, and an automatic stop when the computer runs too hot." },
  { t: "Terminal too", d: "A CLI, a live terminal dashboard and a background service for Linux, macOS and WSL2, driving the same engine." },
  { t: "Demo mode", d: "Explore the app with simulated data, kept apart from real data, without starting any miner." },
];

export default function Home() {
  return (
    <>
      <section className="mx-auto max-w-6xl px-4 pt-20 pb-16 text-center sm:px-6 sm:pt-28">
        <p className="mb-4 text-sm font-medium tracking-widest text-accent uppercase">Erebor</p>
        <h1 className="mx-auto max-w-3xl text-4xl font-semibold tracking-tight sm:text-6xl">{site.tagline}</h1>
        <p className="mx-auto mt-6 max-w-2xl text-lg leading-relaxed text-muted">
          A source-available, local-first mining manager for your own computer. It picks the coin, runs verified miners and shows
          what it really earns.
        </p>
        <div className="mt-10 flex flex-wrap justify-center gap-3">
          <Link href="/download" className="inline-flex h-11 items-center rounded-lg bg-accent-solid px-6 font-medium text-white hover:brightness-110">
            Download Erebor
          </Link>
          <Link href="/#how-it-works" className="inline-flex h-11 items-center rounded-lg border border-line px-6 font-medium hover:border-faint">
            How it works
          </Link>
        </div>
        <p className="mt-4 text-xs text-faint">Windows · macOS · Linux · Source available · No account</p>
      </section>

      <section className="mx-auto max-w-6xl px-4 sm:px-6" aria-label="Preview">
        <figure className="overflow-hidden rounded-2xl border border-line bg-surface shadow-2xl shadow-black/40">
          {/* eslint-disable-next-line @next/next/no-img-element */}
          <img src="/screenshot-dashboard-demo.jpg" alt="Erebor dashboard while mining, in demo mode" width={1326} height={796} className="w-full" />
        </figure>
        <figcaption className="mt-3 text-center text-xs text-faint">Screenshot in demo mode: the values are simulated.</figcaption>
      </section>

      <section className="mx-auto max-w-6xl px-4 pt-24 sm:px-6">
        <div className="grid gap-10 md:grid-cols-2">
          <div>
            <h2 className="text-2xl font-semibold tracking-tight">What Erebor is</h2>
            <p className="mt-4 leading-relaxed text-muted">
              Mining from a home computer sounds simple and isn&apos;t: pick a coin among dozens, find a pool, download a miner
              you hope is the real one, write its command line, keep up as profitability shifts. Erebor is a mining manager,
              not a new miner or a wallet: it does that work for you, in the open, and shows clearly what your computer does,
              what it uses and what it could earn.
            </p>
          </div>
          <div className="rounded-xl border border-line bg-surface p-6">
            <h3 className="font-semibold">At a glance</h3>
            <ul className="mt-3 space-y-2 text-sm text-muted">
              <li>— Mines with your graphics card (NVIDIA/AMD) or your processor</li>
              <li>— Earnings in BTC on your Kryptex account</li>
              <li>— Data and settings stay on your computer</li>
              <li>— Declared Erebor fee: {site.devFeePercent}% of mining time</li>
            </ul>
          </div>
        </div>
      </section>

      <section id="how-it-works" className="mx-auto max-w-6xl scroll-mt-20 px-4 pt-24 sm:px-6">
        <h2 className="text-2xl font-semibold tracking-tight">How it works</h2>
        <ol className="mt-8 grid gap-4 sm:grid-cols-2 lg:grid-cols-4">
          {steps.map((s, i) => (
            <li key={s.t} className="rounded-xl border border-line bg-surface p-5">
              <span className="text-sm font-semibold text-accent">{String(i + 1).padStart(2, "0")}</span>
              <h3 className="mt-3 font-medium">{s.t}</h3>
              <p className="mt-2 text-sm leading-relaxed text-muted">{s.d}</p>
            </li>
          ))}
        </ol>
      </section>

      <section id="features" className="mx-auto max-w-6xl scroll-mt-20 px-4 pt-24 sm:px-6">
        <h2 className="text-2xl font-semibold tracking-tight">Features</h2>
        <div className="mt-8 grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
          {features.map((f) => (
            <div key={f.t} className="rounded-xl border border-line bg-surface p-5">
              <h3 className="font-medium">{f.t}</h3>
              <p className="mt-2 text-sm leading-relaxed text-muted">{f.d}</p>
            </div>
          ))}
        </div>
      </section>

      <section className="mx-auto grid max-w-6xl gap-4 px-4 pt-24 sm:px-6 md:grid-cols-2">
        <div className="rounded-xl border border-line bg-surface p-6">
          <h2 className="text-xl font-semibold">Privacy, non-custodial</h2>
          <p className="mt-3 text-sm leading-relaxed text-muted">
            Erebor never receives your earnings, handles no withdrawals and never touches private keys. No accounts, no
            telemetry: settings and statistics stay on your computer.
          </p>
          <Link href="/privacy" className="mt-4 inline-block text-sm text-accent">Privacy policy →</Link>
        </div>
        <div className="rounded-xl border border-line bg-surface p-6">
          <h2 className="text-xl font-semibold">Transparent fees</h2>
          <p className="mt-3 text-sm leading-relaxed text-muted">
            Erebor is free: {site.devFeePercent}% of effective mining time supports the project, and nothing is ever taken
            from your balance. Miner and pool fees come on top, and the app shows them all.
          </p>
          <Link href="/fees" className="mt-4 inline-block text-sm text-accent">Fee details →</Link>
        </div>
      </section>

      <section className="mx-auto max-w-6xl px-4 pt-24 sm:px-6">
        <div className="rounded-2xl border border-line bg-surface p-8 text-center">
          <h2 className="text-2xl font-semibold tracking-tight">Source available</h2>
          <p className="mx-auto mt-3 max-w-2xl text-muted">
            Source code, dependencies and licences are public and verifiable. No obfuscated code, no hidden mining, no start
            without your action.
          </p>
          <div className="mt-6 flex flex-wrap justify-center gap-3">
            <a href={site.repoUrl} rel="noopener" className="inline-flex h-10 items-center rounded-lg border border-line px-5 text-sm hover:border-faint">
              Source on GitHub
            </a>
            <a href={latestRelease} rel="noopener" className="inline-flex h-10 items-center rounded-lg border border-line px-5 text-sm hover:border-faint">
              Latest release
            </a>
            <Link href="/faq" className="inline-flex h-10 items-center rounded-lg border border-line px-5 text-sm hover:border-faint">
              FAQ
            </Link>
          </div>
        </div>
      </section>
    </>
  );
}

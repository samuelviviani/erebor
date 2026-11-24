import Link from "next/link";
import { release, site } from "@/site";

const steps = [
  { t: "Installa Erebor", d: "Scarica l'applicazione e verifica l'hash del file. Nessun account richiesto." },
  { t: "Dove ricevi i guadagni", d: "Incolla il tuo ID Kryptex: ricevi in Bitcoin sul tuo account Kryptex. Erebor non chiede mai seed phrase o chiavi private." },
  { t: "Il PC si prepara da solo", d: "Erebor scarica il miner ufficiale e ne verifica l'integrità, senza domande tecniche." },
  { t: "Avvia quando vuoi", d: "Il mining parte solo quando premi Avvia e si ferma quando vuoi tu." },
];

const features = [
  { t: "Dashboard essenziale", d: "Stato del mining, hashrate reale, ricompense stimate e commissioni in una sola schermata." },
  { t: "Consumi e costi", d: "Imposta il prezzo dell'elettricità: Erebor stima costi e profitto netto, e dichiara quando i dati non bastano." },
  { t: "Sicurezza hardware", d: "Preset Eco, Bilanciata e Prestazioni, soglia di temperatura con arresto automatico dove il sistema la espone." },
  { t: "Statistiche locali", d: "Storico di hashrate, share, sessioni e ricompense stimate, salvato solo sul tuo computer." },
  { t: "Programmazione", d: "Fasce orarie di mining e riavvio dopo un crash, solo se lo attivi tu." },
  { t: "Modalità demo", d: "Esplora l'app con dati simulati, separati da quelli reali, senza avviare alcun miner." },
];

export default function Home() {
  return (
    <>
      <section className="mx-auto max-w-6xl px-4 pt-20 pb-16 text-center sm:px-6 sm:pt-28">
        <p className="mb-4 text-sm font-medium tracking-widest text-accent uppercase">Erebor</p>
        <h1 className="mx-auto max-w-3xl text-4xl font-semibold tracking-tight sm:text-6xl">Mining, under your control.</h1>
        <p className="mx-auto mt-6 max-w-2xl text-lg leading-relaxed text-muted">
          Gestisci il mining dal tuo computer. Monitora prestazioni, consumi e ricompense, mantenendo il controllo del tuo
          wallet.
        </p>
        <div className="mt-10 flex flex-wrap justify-center gap-3">
          <Link href="/download" className="inline-flex h-11 items-center rounded-lg bg-accent-solid px-6 font-medium text-white hover:brightness-110">
            Scarica Erebor
          </Link>
          <Link href="/#come-funziona" className="inline-flex h-11 items-center rounded-lg border border-line px-6 font-medium hover:border-faint">
            Scopri come funziona
          </Link>
        </div>
        <p className="mt-4 text-xs text-faint">
          {release.version ? `Versione ${release.version}` : "Prima release in preparazione"} · Open source · Nessun account
        </p>
      </section>

      <section className="mx-auto max-w-6xl px-4 sm:px-6" aria-label="Anteprima">
        <figure className="overflow-hidden rounded-2xl border border-line bg-surface shadow-2xl shadow-black/40">
          {/* eslint-disable-next-line @next/next/no-img-element */}
          <img src="/screenshot-dashboard-demo.jpg" alt="Dashboard di Erebor con mining attivo, in modalità demo" width={1326} height={796} className="w-full" />
        </figure>
        <figcaption className="mt-3 text-center text-xs text-faint">Screenshot in modalità demo: i valori mostrati sono simulati.</figcaption>
      </section>

      <section className="mx-auto max-w-6xl px-4 pt-24 sm:px-6">
        <div className="grid gap-10 md:grid-cols-2">
          <div>
            <h2 className="text-2xl font-semibold tracking-tight">Cos&apos;è Erebor</h2>
            <p className="mt-4 leading-relaxed text-muted">
              Erebor è un mining manager: non è un nuovo miner né un portafoglio. Avvia e controlla{" "}
              <a href="https://xmrig.com" className="text-accent" rel="noopener">XMRig</a>, un miner open source, e ti mostra
              in modo chiaro cosa succede: quanto lavora il tuo computer, quanto consuma, quanto potrebbe rendere e quali
              commissioni si applicano.
            </p>
          </div>
          <div className="rounded-xl border border-line bg-surface p-6">
            <h3 className="font-semibold">In breve</h3>
            <ul className="mt-3 space-y-2 text-sm text-muted">
              <li>— Mining con scheda video (SRBMiner) o processore (XMRig)</li>
              <li>— Ricompense in BTC sul tuo account Kryptex</li>
              <li>— Dati e configurazione solo sul tuo computer</li>
              <li>— Commissione Erebor dichiarata: {site.devFeePercent}% del tempo di mining</li>
            </ul>
          </div>
        </div>
      </section>

      <section id="come-funziona" className="mx-auto max-w-6xl scroll-mt-20 px-4 pt-24 sm:px-6">
        <h2 className="text-2xl font-semibold tracking-tight">Come funziona</h2>
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

      <section id="funzionalita" className="mx-auto max-w-6xl scroll-mt-20 px-4 pt-24 sm:px-6">
        <h2 className="text-2xl font-semibold tracking-tight">Funzionalità</h2>
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
          <h2 className="text-xl font-semibold">Privacy e modello non-custodial</h2>
          <p className="mt-3 text-sm leading-relaxed text-muted">
            Erebor non riceve le tue ricompense, non gestisce prelievi e non accede a chiavi private. Non esistono account né
            telemetria: configurazione e statistiche restano sul tuo computer.
          </p>
          <Link href="/privacy" className="mt-4 inline-block text-sm text-accent">Informativa privacy →</Link>
        </div>
        <div className="rounded-xl border border-line bg-surface p-6">
          <h2 className="text-xl font-semibold">Commissioni trasparenti</h2>
          <p className="mt-3 text-sm leading-relaxed text-muted">
            Erebor è gratuito: il {site.devFeePercent}% del tempo di mining effettivo sostiene il progetto, senza prelievi dal
            tuo saldo. A questo si aggiungono la donazione integrata in XMRig ({site.xmrigDonatePercent}%) e la fee della
            pool, tutte indicate nell&apos;app.
          </p>
          <Link href="/fees" className="mt-4 inline-block text-sm text-accent">Dettagli sulle commissioni →</Link>
        </div>
      </section>

      <section className="mx-auto max-w-6xl px-4 pt-24 sm:px-6">
        <h2 className="text-2xl font-semibold tracking-tight">Sistemi operativi</h2>
        <p className="mt-3 max-w-2xl text-muted">
          Obiettivo: Windows 10/11 (x64) e Linux (Ubuntu x64), con macOS in una fase successiva. La pagina download elenca
          soltanto le piattaforme con una release effettivamente pubblicata.
        </p>
        <Link href="/download" className="mt-4 inline-block text-sm text-accent">Vai ai download →</Link>
      </section>

      <section className="mx-auto max-w-6xl px-4 pt-24 sm:px-6">
        <div className="rounded-2xl border border-line bg-surface p-8 text-center">
          <h2 className="text-2xl font-semibold tracking-tight">Open source</h2>
          <p className="mx-auto mt-3 max-w-2xl text-muted">
            Codice sorgente, dipendenze e licenze sono pubblici e verificabili. Nessun codice offuscato, nessun mining
            nascosto, nessun avvio senza consenso.
          </p>
          <div className="mt-6 flex flex-wrap justify-center gap-3">
            <Link href="/open-source" className="inline-flex h-10 items-center rounded-lg border border-line px-5 text-sm hover:border-faint">
              Approfondisci
            </Link>
            <Link href="/faq" className="inline-flex h-10 items-center rounded-lg border border-line px-5 text-sm hover:border-faint">
              Domande frequenti
            </Link>
          </div>
        </div>
      </section>
    </>
  );
}

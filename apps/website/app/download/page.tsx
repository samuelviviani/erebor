import type { Metadata } from "next";
import Link from "next/link";
import { Doc } from "@/components/chrome";
import { release, site } from "@/site";

export const metadata: Metadata = { title: "Download", alternates: { canonical: "/download" } };

export default function Download() {
  return (
    <Doc title="Scarica Erebor" lead="Sono elencate solo le piattaforme con una release realmente pubblicata e verificabile.">
      {release.assets.length === 0 ? (
        <div className="mt-8 rounded-xl border border-line bg-surface p-6">
          <h2 className="!mt-0">Nessuna release pubblicata</h2>
          <p>
            La prima release è in preparazione. Nel frattempo puoi compilare Erebor dai sorgenti seguendo la guida di
            sviluppo{site.repoUrl ? " nel repository" : ""}.
          </p>
          <p>
            <Link href="/docs#compilare">Come compilare Erebor →</Link>
          </p>
        </div>
      ) : (
        <>
          <p>
            Versione <code>{release.version}</code>
            {release.date && ` · ${release.date}`}
          </p>
          <div className="mt-6 space-y-3">
            {release.assets.map((a) => (
              <div key={a.file} className="rounded-xl border border-line bg-surface p-5">
                <div className="flex flex-wrap items-center justify-between gap-3">
                  <div>
                    <div className="font-medium text-fg">{a.label}</div>
                    <div className="text-xs text-faint">
                      {a.file} · {(a.size / 1e6).toFixed(1)} MB
                    </div>
                  </div>
                  <a href={a.url} className="inline-flex h-9 items-center rounded-lg bg-accent-solid px-4 text-sm font-medium !text-white">
                    Scarica
                  </a>
                </div>
                <p className="!mb-0 mt-3 break-all font-mono text-xs">SHA-256: {a.sha256}</p>
              </div>
            ))}
          </div>
        </>
      )}
      <h2>Verifica il download</h2>
      <p>Prima di installare, confronta l&apos;hash del file con quello pubblicato qui e nella release GitHub:</p>
      <pre>
        <code>{`# Linux / macOS
sha256sum Erebor_*.AppImage

# Windows (PowerShell)
Get-FileHash .\\Erebor_*_x64-setup.exe -Algorithm SHA256`}</code>
      </pre>
      <h2>Requisiti</h2>
      <ul>
        <li>Windows 10/11 x64 (WebView2, già incluso in Windows 11) oppure Linux x64 (Ubuntu 22.04 o successivo, WebKitGTK 4.1).</li>
        <li>CPU x64; almeno 4 GB di RAM consigliati (RandomX usa circa 2 GB in modalità veloce).</li>
        <li>Connessione Internet per la pool e, al primo avvio, per scaricare XMRig da GitHub.</li>
      </ul>
      <h2>Antivirus</h2>
      <p>
        Alcuni antivirus segnalano XMRig come &quot;riskware&quot; perché i miner vengono spesso installati di nascosto da
        malware. Erebor non tenta di aggirare antivirus o controlli di sicurezza: se XMRig viene bloccato, valuta tu se
        autorizzarlo, dopo averne verificato l&apos;origine.
      </p>
    </Doc>
  );
}

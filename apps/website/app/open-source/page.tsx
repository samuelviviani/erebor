import type { Metadata } from "next";
import { Doc } from "@/components/chrome";
import { site } from "@/site";

export const metadata: Metadata = { title: "Open source", alternates: { canonical: "/open-source" } };

export default function OpenSource() {
  return (
    <Doc title="Open source" lead="Tutto ciò che Erebor fa sul tuo computer è verificabile nel codice sorgente.">
      <ul className="mt-6">
        <li>Licenza di Erebor: MIT.</li>
        <li>XMRig (GPLv3) non è incluso nel pacchetto: viene scaricato dalla release ufficiale fissata e verificato con SHA-256.</li>
        <li>Le licenze delle dipendenze sono elencate in <code>THIRD_PARTY_NOTICES.md</code>.</li>
        <li>Nessun codice offuscato, nessuna persistenza nascosta, nessun avvio senza consenso.</li>
        <li>Build e release avvengono tramite GitHub Actions pubbliche; i download riportano l&apos;hash SHA-256.</li>
      </ul>
      {site.repoUrl ? (
        <p>
          Repository: <a href={site.repoUrl} rel="noopener">{site.repoUrl}</a>
        </p>
      ) : (
        <p>Il link al repository sarà pubblicato con la prima release.</p>
      )}
      <h2>Contribuire</h2>
      <p>Leggi <code>CONTRIBUTING.md</code> e <code>DEVELOPMENT.md</code>. Le vulnerabilità vanno segnalate in privato seguendo <code>SECURITY.md</code>.</p>
    </Doc>
  );
}

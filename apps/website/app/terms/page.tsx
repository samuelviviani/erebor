import type { Metadata } from "next";
import { Doc } from "@/components/chrome";

export const metadata: Metadata = { title: "Termini di utilizzo", alternates: { canonical: "/terms" } };

export default function Terms() {
  return (
    <Doc title="Termini di utilizzo" lead="Bozza: non costituisce una revisione legale. Da verificare con un professionista prima della distribuzione commerciale.">
      <h2>Licenza</h2>
      <p>Erebor è software open source distribuito con licenza MIT, &quot;così com&apos;è&quot;, senza garanzie di alcun tipo. XMRig è un progetto separato con licenza GPLv3.</p>
      <h2>Uso responsabile</h2>
      <ul>
        <li>Usa Erebor solo su hardware di tua proprietà o per cui hai un&apos;autorizzazione esplicita.</li>
        <li>Il mining aumenta consumi elettrici, calore e usura dell&apos;hardware: ne sei responsabile.</li>
        <li>Verifica che il mining di criptovalute sia consentito dalle leggi, dai contratti e dalle regole fiscali che ti riguardano.</li>
      </ul>
      <h2>Nessuna promessa di rendimento</h2>
      <p>Le ricompense mostrate sono stime. I pagamenti dipendono esclusivamente dalla pool. Erebor non garantisce rendimenti e non fornisce consulenza finanziaria.</p>
      <h2>Commissioni</h2>
      <p>Le commissioni applicate sono descritte nella pagina Commissioni e indicate nell&apos;app (schermata iniziale e Impostazioni › Commissioni).</p>
    </Doc>
  );
}

import type { Metadata } from "next";
import { Doc } from "@/components/chrome";

export const metadata: Metadata = { title: "Privacy Policy", alternates: { canonical: "/privacy" } };

export default function Privacy() {
  return (
    <Doc title="Privacy Policy" lead="Bozza tecnica: descrive il comportamento reale del software. Deve essere rivista da un professionista legale prima della pubblicazione commerciale.">
      <h2>Applicazione desktop</h2>
      <ul>
        <li>Nessun account, nessuna telemetria, nessun analytics.</li>
        <li>Configurazione (indirizzo wallet pubblico, pool, preferenze) e statistiche sono salvate solo sul tuo computer, nella cartella dati dell&apos;applicazione indicata nelle impostazioni.</li>
        <li>Puoi esportare la configurazione o eliminare tutti i dati locali in qualsiasi momento dalle impostazioni.</li>
      </ul>
      <h2>Servizi esterni contattati</h2>
      <ul>
        <li><strong>Pool di mining</strong> scelta da te: riceve il tuo indirizzo pubblico, il nome worker e il tuo indirizzo IP (necessari al mining). Se la pool offre un&apos;API pubblica, Erebor la interroga con il tuo indirizzo pubblico per mostrare saldo e pagamenti.</li>
        <li><strong>CoinGecko</strong>: prezzo di XMR in EUR/USD (nessun dato personale oltre all&apos;indirizzo IP).</li>
        <li><strong>GitHub</strong>: solo quando installi XMRig, per scaricare la release ufficiale.</li>
        <li><strong>Server di donazione di XMRig</strong>: per la percentuale di donazione integrata nel miner.</li>
      </ul>
      <p>Questi servizi hanno informative proprie. Erebor non usa Tor o proxy di anonimizzazione.</p>
      <h2>Sito web</h2>
      <p>Il sito è statico: non usa cookie, analytics o moduli. Il provider di hosting può registrare log tecnici (es. indirizzo IP) secondo la propria informativa.</p>
      <h2>Contatti</h2>
      <p>Per segnalazioni di sicurezza consulta il file <code>SECURITY.md</code> nel repository.</p>
    </Doc>
  );
}

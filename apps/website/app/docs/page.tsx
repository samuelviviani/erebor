import type { Metadata } from "next";
import { Doc } from "@/components/chrome";

export const metadata: Metadata = { title: "Documentazione", alternates: { canonical: "/docs" } };

export default function Docs() {
  return (
    <Doc title="Documentazione">
      <h2 id="primo-avvio">Primo avvio</h2>
      <p>Erebor guida la configurazione in tre passaggi:</p>
      <ul>
        <li><strong>Dove ricevi i guadagni</strong>: il tuo <em>ID Kryptex</em> (<code>krx…</code>). I guadagni arrivano in BTC sul tuo account Kryptex, per tutte le monete. Altri wallet arriveranno in seguito.</li>
        <li><strong>Preparazione del PC</strong>: rilevamento dell&apos;hardware e installazione automatica di XMRig verificato. Nelle opzioni avanzate puoi selezionare un binario esistente.</li>
        <li><strong>Pronto</strong>: il mining parte quando premi Inizia.</li>
      </ul>
      <p>Pool personalizzate, prestazioni, sicurezza ed energia si configurano in seguito, nelle Impostazioni.</p>
      <h2 id="modalita">Modalità di utilizzo</h2>
      <ul>
        <li><strong>Eco</strong>: 25% dei thread, priorità minima.</li>
        <li><strong>Bilanciata</strong>: 50% dei thread, priorità bassa.</li>
        <li><strong>Prestazioni</strong>: 100% dei thread.</li>
        <li><strong>Personalizzata</strong>: percentuale a scelta.</li>
      </ul>
      <p>Le modalità impostano l&apos;opzione <code>cpu.max-threads-hint</code> e la priorità del processo di XMRig. Erebor non esegue overclock e non modifica BIOS, firmware o driver.</p>
      <h2 id="stime">Stime e costi</h2>
      <p>
        Ricompense stimate = hashrate × tempo × ricompensa del blocco ÷ difficoltà di rete. Da qui si sottraggono la
        donazione XMRig e la fee della pool. Costo energia = potenza media (kW) × ore × prezzo al kWh. Quando la potenza non
        è misurabile, Erebor usa la stima che inserisci tu e lo indica chiaramente.
      </p>
      <h2 id="sicurezza">Sicurezza</h2>
      <ul>
        <li>L&apos;API di XMRig ascolta solo su <code>127.0.0.1</code> con un token casuale per sessione.</li>
        <li>XMRig viene avviato con argomenti fissi, senza shell.</li>
        <li>Soglia di temperatura con arresto automatico, dove il sistema espone la temperatura.</li>
        <li>Se Erebor si chiude in modo anomalo, al riavvio successivo termina l&apos;eventuale miner rimasto attivo.</li>
      </ul>
      <h2 id="compilare">Compilare dai sorgenti</h2>
      <pre>
        <code>{`# Requisiti: Node.js 20+, pnpm 10, Rust stable, dipendenze di sistema Tauri 2
pnpm install
pnpm --filter @erebor/desktop tauri dev     # sviluppo
pnpm --filter @erebor/desktop tauri build   # pacchetti`}</code>
      </pre>
      <p>Dettagli in <code>DEVELOPMENT.md</code> e <code>INSTALLATION.md</code> nel repository.</p>
    </Doc>
  );
}

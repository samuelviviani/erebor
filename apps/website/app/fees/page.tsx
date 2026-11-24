import type { Metadata } from "next";
import { Doc } from "@/components/chrome";
import { site } from "@/site";

export const metadata: Metadata = { title: "Commissioni", alternates: { canonical: "/fees" } };

export default function Fees() {
  return (
    <Doc title="Commissioni" lead="Erebor è gratuito. Qui trovi tutte le voci di costo: nell'app sono riassunte in una riga e consultabili in Impostazioni › Commissioni.">
      <div className="mt-8 overflow-x-auto rounded-xl border border-line">
        <table className="w-full text-sm">
          <thead className="bg-surface text-left text-muted">
            <tr>
              <th className="p-3 font-medium">Voce</th>
              <th className="p-3 font-medium">Valore</th>
              <th className="p-3 font-medium">Chi la riceve</th>
            </tr>
          </thead>
          <tbody className="text-muted">
            <tr className="border-t border-line">
              <td className="p-3 text-fg">Commissione Erebor</td>
              <td className="p-3">{site.devFeePercent}% del tempo di mining effettivo</td>
              <td className="p-3">Progetto Erebor, sull&apos;account Kryptex del progetto con il worker &quot;EREBOR&quot;</td>
            </tr>
            <tr className="border-t border-line">
              <td className="p-3 text-fg">Donazione XMRig</td>
              <td className="p-3">{site.xmrigDonatePercent}% del tempo (minimo delle build ufficiali)</td>
              <td className="p-3">Sviluppatori di XMRig</td>
            </tr>
            <tr className="border-t border-line">
              <td className="p-3 text-fg">Commissione SRBMiner (solo scheda video)</td>
              <td className="p-3">0,65–3% secondo l&apos;algoritmo (es. Pearl 2%)</td>
              <td className="p-3">Sviluppatore di SRBMiner-MULTI</td>
            </tr>
            <tr className="border-t border-line">
              <td className="p-3 text-fg">Fee della pool</td>
              <td className="p-3">Dipende dalla pool e dalla moneta (Kryptex: 1% per la maggior parte, Pearl 3%; unMineable: circa 1%; SupportXMR: 0,6%)</td>
              <td className="p-3">La pool</td>
            </tr>
          </tbody>
        </table>
      </div>
      <h2>Come funziona la commissione Erebor</h2>
      <ul>
        <li>Nulla viene mai prelevato dal tuo wallet: Erebor non ha accesso ai tuoi fondi.</li>
        <li>
          Per circa 2 minuti ogni 100 minuti di mining effettivo, il miner lavora per l&apos;account del progetto. Turni da 2
          minuti invece di tanti brevi cambi riducono il tempo perso nelle riconnessioni alla pool.
        </li>
        <li>Conta solo il tempo in cui il miner sta effettivamente calcolando: pause, arresti e periodi di inattività non generano commissioni.</li>
        <li>La contabilità del tempo è salvata localmente: un riavvio non azzera né raddoppia la commissione.</li>
        <li>L&apos;app mostra in tempo reale quando sta lavorando per il progetto e quanto tempo gli è stato dedicato.</li>
        <li>Il tempo del progetto è registrato con un unico nome di worker, uguale per tutti: il progetto non vede i nomi dei tuoi computer.</li>
        <li>Il mining parte solo quando premi Avvia e puoi fermarlo in qualsiasi momento.</li>
        <li>L&apos;indirizzo del progetto è un parametro di compilazione pubblico e verificabile. Le build senza indirizzo configurato non applicano alcuna commissione.</li>
      </ul>
      <h2>La donazione di XMRig</h2>
      <p>
        XMRig include una donazione ai propri sviluppatori, regolata dall&apos;opzione <code>donate-level</code>: per quella
        percentuale di tempo il miner si collega a un server di donazione. Le build ufficiali impongono un minimo
        dell&apos;1%. È una voce diversa e indipendente dalla commissione Erebor.
      </p>
      <h2>Costo complessivo</h2>
      <p>
        Con la pool proposta (Kryptex) il costo dichiarato è circa {site.devFeePercent}% + {site.xmrigDonatePercent}% del
        tempo + 1% delle ricompense. Le statistiche di Erebor separano ricavi lordi, donazione XMRig, fee della pool, costi
        energetici e risultato netto, senza sottrarre due volte la stessa voce.
      </p>
    </Doc>
  );
}

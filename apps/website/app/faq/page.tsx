import type { Metadata } from "next";
import { Doc } from "@/components/chrome";

export const metadata: Metadata = { title: "FAQ", alternates: { canonical: "/faq" } };

const faq = [
  ["Erebor mi farà guadagnare?", "Non necessariamente. Il mining con CPU produce piccole quantità di Monero e i costi elettrici possono superare le ricompense. Erebor stima ricavi, costi e profitto netto e ti dice quando i dati non bastano per una stima affidabile. Non promettiamo alcun rendimento."],
  ["Erebor custodisce i miei fondi?", "No. Le ricompense sono pagate dalla pool direttamente al tuo indirizzo, secondo le regole e la soglia minima della pool. Erebor non gestisce prelievi e non conosce le tue chiavi."],
  ["Perché mi serve solo l'indirizzo pubblico?", "Per ricevere pagamenti basta l'indirizzo. Seed phrase e chiavi private servono a spendere: Erebor non li chiede mai e rifiuta gli input che sembrano segreti."],
  ["Quali criptovalute sono supportate?", "Con la scheda video (NVIDIA o AMD, su Windows e Linux): Pearl, Ethereum Classic, EthereumPoW, Ergo, Ravencoin e Xelis, tramite SRBMiner-MULTI. Con la CPU: Monero tramite XMRig. Tutte si minano su Kryptex e i guadagni ti arrivano in Bitcoin sul tuo account Kryptex."],
  ["Come sceglie Erebor cosa minare?", "In modalità automatica Erebor prova per qualche minuto ogni moneta compatibile con il tuo computer, misura la velocità reale e poi mina quella che rende di più secondo i dati di mercato (WhatToMine, Kryptex), ricontrollando ogni mezz'ora. Puoi anche scegliere tu la moneta nella pagina Mining."],
  ["Posso ricevere Bitcoin senza minare Bitcoin?", "Sì, ed è il modo consigliato: minare Bitcoin con un computer normale non conviene. Il tuo computer mina la moneta più adatta e la pool la converte in BTC. Con Kryptex i BTC arrivano sul tuo account Kryptex; con l'opzione \"Il mio wallet Bitcoin\" arrivano direttamente sul tuo wallet (Tangem, Ledger, Electrum…) tramite unMineable, una volta al giorno sopra 0,00075 BTC."],
  ["Kryptex custodisce i miei guadagni?", "Sì: con l'ID Kryptex il saldo resta sul tuo account Kryptex finché non prelevi. Erebor non ha accesso a quell'account. Se preferisci ricevere direttamente su un tuo wallet, scegli \"Il mio wallet Bitcoin\" (tramite unMineable) oppure un indirizzo Monero."],
  ["Posso usare la GPU?", "Sì: se Erebor trova una scheda video NVIDIA o AMD e hai un ID Kryptex, installa SRBMiner-MULTI (programma gratuito ma non open source, con una sua commissione dallo 0,65% al 3% secondo l'algoritmo) e mina con la scheda video. Su macOS il mining GPU non è disponibile."],
  ["Il mining parte da solo?", "No. Parte solo quando premi Avvia, oppure nella fascia oraria che hai configurato tu. L'avvio con il sistema operativo non è attivo. Chiudendo Erebor il mining si arresta."],
  ["Che differenza c'è tra stima e pagamento?", "Le ricompense stimate derivano da hashrate e difficoltà di rete. Il saldo non pagato e i pagamenti confermati arrivano dall'API della pool, quando disponibile. Erebor li mostra sempre separati."],
  ["Il mio antivirus segnala XMRig.", "È comune: molti malware usano XMRig di nascosto. Erebor scarica solo la release ufficiale fissata e ne verifica l'hash; non tenta di aggirare l'antivirus. La decisione resta tua."],
  ["Erebor raccoglie dati?", "No. Non esistono account, telemetria o analytics. Erebor contatta solo la pool, CoinGecko per il prezzo di XMR e GitHub per installare XMRig."],
];

export default function Faq() {
  return (
    <Doc title="Domande frequenti">
      <div className="mt-8 space-y-3">
        {faq.map(([q, a]) => (
          <details key={q} className="group rounded-xl border border-line bg-surface p-5">
            <summary className="cursor-pointer list-none font-medium text-fg">{q}</summary>
            <p className="!mb-0 mt-3 text-sm">{a}</p>
          </details>
        ))}
      </div>
    </Doc>
  );
}

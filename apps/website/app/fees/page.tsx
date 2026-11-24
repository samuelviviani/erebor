import type { Metadata } from "next";
import { Doc } from "@/components/chrome";
import { site } from "@/site";

export const metadata: Metadata = { title: "Fees", alternates: { canonical: "/fees" } };

const fees = [
  ["Erebor fee", `${site.devFeePercent}% of effective mining time`, "The Erebor project, on its Kryptex account with the worker \"EREBOR\""],
  ["XMRig donation (processor)", `${site.xmrigDonatePercent}% of time (the minimum of official builds)`, "XMRig developers"],
  ["SRBMiner-MULTI fee (graphics card)", "0.65–3% depending on the algorithm (e.g. Pearl 2%)", "SRBMiner-MULTI developer"],
  ["lolMiner fee (graphics card)", "0.75–2% depending on the algorithm", "lolMiner developer"],
  ["Pool fee", "Kryptex: 1% for most coins, Pearl 3%", "The pool"],
];

export default function Fees() {
  return (
    <Doc title="Fees" lead="Erebor is free. This page lists every cost; the app's estimates already deduct them.">
      <div className="mt-8 overflow-x-auto rounded-xl border border-line">
        <table className="w-full text-sm">
          <thead className="bg-surface text-left text-muted">
            <tr>
              <th className="p-3 font-medium">Item</th>
              <th className="p-3 font-medium">Amount</th>
              <th className="p-3 font-medium">Paid to</th>
            </tr>
          </thead>
          <tbody className="text-muted">
            {fees.map(([item, amount, to]) => (
              <tr key={item} className="border-t border-line">
                <td className="p-3 text-fg">{item}</td>
                <td className="p-3">{amount}</td>
                <td className="p-3">{to}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      <p className="text-sm">cpuminer-opt has no fee. Only the miner in use applies its own fee.</p>
      <h2>How the Erebor fee works</h2>
      <ul>
        <li>Nothing is ever taken from your wallet: Erebor has no access to your funds.</li>
        <li>
          For about 2 minutes every 100 minutes of effective mining, the miner works for the project&apos;s account. Two-minute
          blocks instead of many short switches waste less time reconnecting to the pool.
        </li>
        <li>Only time the miner is actually hashing counts: pauses, stops and idle time generate no fee.</li>
        <li>The time ledger is saved locally: a restart neither resets nor doubles the fee.</li>
        <li>The app shows live when it is working for the project.</li>
        <li>Project time uses a single worker name, the same for everyone: the project never sees your computers&apos; names.</li>
        <li>Mining starts only when you press Start and you can stop it at any time.</li>
        <li>The project&apos;s account is a public build parameter. Builds without it apply no fee.</li>
      </ul>
      <h2>Miner fees</h2>
      <p>
        Miners keep a share of time for their developers. XMRig&apos;s is set by its <code>donate-level</code> option, with a 1%
        minimum in official builds; SRBMiner-MULTI and lolMiner set theirs per algorithm. They are separate from, and
        independent of, the Erebor fee.
      </p>
      <h2>Total cost</h2>
      <p>
        With Kryptex, mining Monero costs about {site.devFeePercent}% + {site.xmrigDonatePercent}% of time plus 1% of the
        rewards. Erebor&apos;s statistics separate gross earnings, fees, electricity and net result, without subtracting the
        same item twice.
      </p>
    </Doc>
  );
}

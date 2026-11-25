import type { Metadata } from "next";
import { Doc } from "@/components/chrome";
import { site } from "@/site";

export const metadata: Metadata = { title: "Fees", alternates: { canonical: "/fees" } };

const fees = [
  ["Erebor fee", `${site.devFeePercent}% of effective mining time`, "The Erebor project (worker \"EREBOR\"): its Bitcoin address on unMineable (for coins unMineable doesn't list, through another coin of the same algorithm), or its Kryptex account"],
  ["Donation (optional)", "0% by default; you can add 1–3% of mining time in Settings › Support Erebor", "The Erebor project, only if you turn it on"],
  ["XMRig donation (processor)", `${site.xmrigDonatePercent}% of time (the minimum of official builds)`, "XMRig developers"],
  ["SRBMiner-MULTI fee (graphics card)", "0.65–3% depending on the algorithm (e.g. Pearl 2%)", "SRBMiner-MULTI developer"],
  ["lolMiner fee (graphics card)", "0.75–2% depending on the algorithm", "lolMiner developer"],
  ["Pool fee", "unMineable: 1%; Kryptex: 1% for most coins, Pearl 3%; your own pool: its own fee (shown in the app)", "The pool"],
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
      <h2>Referrals</h2>
      <ul>
        <li>
          When you are paid through unMineable, Erebor adds its referral code (<code>v3ni-3jdo</code>) to your login. Per
          unMineable this lowers your pool fee, and unMineable pays the project a small share out of its own fee. It costs
          you nothing.
        </li>
        <li>
          The &quot;create a Kryptex account&quot; link in the app is Erebor&apos;s Kryptex Pool referral link. Kryptex pays the
          project part of its own pool fee for accounts created through it; you pay nothing extra.
        </li>
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

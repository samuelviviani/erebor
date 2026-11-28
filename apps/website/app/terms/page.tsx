import type { Metadata } from "next";
import { Doc } from "@/components/chrome";

export const metadata: Metadata = { title: "Terms of use", alternates: { canonical: "/terms" } };

export default function Terms() {
  return (
    <Doc title="Terms of use" lead="Draft, not a legal review. To be checked by a professional before commercial distribution.">
      <h2>Licence</h2>
      <p>Erebor is distributed under the Erebor Source-Available License (ESAL). See the repository LICENSE.md for its terms. The miners it runs are separate projects under their own licences.</p>
      <h2>Responsible use</h2>
      <ul>
        <li>Use Erebor only on hardware you own or are explicitly allowed to use.</li>
        <li>Mining increases power use, heat and hardware wear: that is your responsibility.</li>
        <li>Check that cryptocurrency mining is allowed by the laws, contracts and tax rules that apply to you.</li>
      </ul>
      <h2>No promised return</h2>
      <p>Earnings shown are estimates. Payments depend solely on the pool. Erebor guarantees no return and gives no financial advice.</p>
      <h2>Fees</h2>
      <p>The fees that apply are described on the Fees page and shown in the app.</p>
    </Doc>
  );
}

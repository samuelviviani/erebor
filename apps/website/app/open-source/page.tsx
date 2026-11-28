import type { Metadata } from "next";
import { Doc } from "@/components/chrome";
import { site } from "@/site";

export const metadata: Metadata = { title: "Source available", alternates: { canonical: "/open-source" } };

export default function OpenSource() {
  return (
    <Doc title="Source available" lead="Everything Erebor does on your computer can be checked in the source code.">
      <ul className="mt-6">
        <li>Erebor&apos;s licence: Erebor Source-Available License (ESAL).</li>
        <li>No miner is bundled: each is downloaded from its official release and verified with a SHA-256 pinned in the repository.</li>
        <li>Dependency licences are listed in <code>THIRD_PARTY_NOTICES.md</code>.</li>
        <li>No obfuscated code, no hidden persistence, no start without your action.</li>
        <li>Every release lists the SHA-256 of its files; app and CLI updates are signed.</li>
      </ul>
      <p>
        Repository: <a href={site.repoUrl} rel="noopener">{site.repoUrl}</a>
      </p>
      <h2>Contributing</h2>
      <p>Read <code>CONTRIBUTING.md</code> and <code>DEVELOPMENT.md</code>. Report vulnerabilities privately, as described in <code>SECURITY.md</code>.</p>
    </Doc>
  );
}

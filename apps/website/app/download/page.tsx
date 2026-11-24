import type { Metadata } from "next";
import { Doc } from "@/components/chrome";
import { installCommand, latestRelease } from "@/site";

export const metadata: Metadata = { title: "Download", alternates: { canonical: "/download" } };

const files = [
  ["Windows 10/11 (x64)", "Erebor_x.y.z_x64-setup.exe"],
  ["macOS (Apple Silicon and Intel)", "Erebor_x.y.z_universal.dmg"],
  ["Linux (Ubuntu 22.04+, x64)", "Erebor_x.y.z_amd64.deb"],
];

export default function Download() {
  return (
    <Doc title="Download Erebor" lead="Every release is published on GitHub, with a SHA-256 for each file. Once installed, Erebor updates itself.">
      <div className="mt-8 rounded-xl border border-line bg-surface p-6">
        <ul className="!my-0">
          {files.map(([system, file]) => (
            <li key={file}>
              <span className="text-fg">{system}</span>: <code>{file}</code>
            </li>
          ))}
        </ul>
        <a href={latestRelease} rel="noopener" className="mt-5 inline-flex h-10 items-center rounded-lg bg-accent-solid px-5 text-sm font-medium !text-white">
          Latest release on GitHub
        </a>
      </div>
      <h2>Command line</h2>
      <p>For Linux, macOS and WSL2: downloads the signed binary, checks its SHA-256, installs it into <code>~/.local/bin</code> (no root) and starts the guided setup.</p>
      <pre>
        <code>{installCommand}</code>
      </pre>
      <h2>Check the download</h2>
      <p>Before installing, compare the file&apos;s hash with the one in <code>SHA256SUMS</code>, attached to the release:</p>
      <pre>
        <code>{`# Linux / macOS
shasum -a 256 Erebor_*

# Windows (PowerShell)
Get-FileHash .\\Erebor_*_x64-setup.exe -Algorithm SHA256`}</code>
      </pre>
      <h2>Requirements</h2>
      <ul>
        <li>Windows 10/11 x64 (WebView2, built into Windows 11), macOS, or Linux x64 (Ubuntu 22.04 or later, WebKitGTK 4.1).</li>
        <li>At least 4 GB of RAM recommended (RandomX uses about 2 GB in fast mode). GPU mining needs an NVIDIA or AMD card on Windows or Linux.</li>
        <li>An internet connection for the pool, market data and the first download of each miner.</li>
      </ul>
      <h2>Antivirus</h2>
      <p>
        Antivirus software often flags miners as &quot;riskware&quot;, because malware installs them secretly. Erebor never tries
        to get around an antivirus. On Windows it explains the warning and, only if you agree, excludes its own miners folder
        from Defender (Windows asks for administrator approval).
      </p>
    </Doc>
  );
}

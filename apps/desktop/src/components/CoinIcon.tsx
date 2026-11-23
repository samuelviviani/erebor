import { useState } from "react";

// Official coin logos (trademarks of their projects, used only to identify the coins),
// downloaded once from CoinGecko and bundled in public/coins: no network at runtime.
const LOGOS = new Set(["XMR", "PRL", "ETC", "ETHW", "ERG", "RVN", "XEL", "BTC"]);

export function CoinIcon({ id, size = 28, className }: { id: string; size?: number; className?: string }) {
  const [broken, setBroken] = useState(false);
  if (LOGOS.has(id) && !broken) {
    return (
      <img
        src={`/coins/${id}.png`}
        width={size}
        height={size}
        alt=""
        aria-hidden
        draggable={false}
        style={{ width: size, height: size, minWidth: size }}
        onError={() => setBroken(true)}
        className={`shrink-0 rounded-full ${className ?? ""}`}
      />
    );
  }
  // Fallback for unknown coins: neutral badge with the ticker initial.
  return (
    <svg width={size} height={size} viewBox="0 0 32 32" aria-hidden className={`shrink-0 ${className ?? ""}`}>
      <circle cx="16" cy="16" r="16" fill="#2e2e2e" />
      <text x="16" y="16" textAnchor="middle" dominantBaseline="central" fill="#ededed" fontSize="15" fontWeight="700" fontFamily="ui-sans-serif, system-ui, sans-serif">
        {id.slice(0, 1)}
      </text>
    </svg>
  );
}

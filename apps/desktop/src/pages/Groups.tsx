import { Play, Plus, Square, Trash2 } from "lucide-react";
import { useState } from "react";
import { ResultsModal, useFleetPolling, useLang } from "../components/fleet";
import { PageTitle } from "../components/Layout";
import { Button, compactInput, ConfirmDialog, cx, Empty } from "../components/ui";
import { errorText, fleet, type DeviceGroup, type OpResult } from "../lib/api";
import { translateError, useT } from "../lib/i18n";
import { useApp } from "../lib/store";

/** Groups ("GPU rigs", "ASIC farm"…): name, electricity price, and actions on all members. */
export default function Groups() {
  const t = useT();
  const lang = useLang();
  useFleetPolling();
  const { fleet: data, refreshFleet, toast } = useApp();
  const [name, setName] = useState("");
  const [results, setResults] = useState<OpResult[] | null>(null);
  const [confirm, setConfirm] = useState<{ g: DeviceGroup; action: "stop" | "delete" } | null>(null);
  const groups = data?.groups ?? [];
  const members = (g: DeviceGroup) => (data?.devices ?? []).filter((d) => d.group === g.id);
  const wrap = async (p: Promise<unknown>) => {
    try {
      const r = await p;
      if (Array.isArray(r)) setResults(r as OpResult[]);
      await refreshFleet();
    } catch (e) {
      toast("error", translateError(lang, errorText(e)));
    }
  };
  return (
    <div className="max-w-3xl">
      <PageTitle sub={t("fleet.groups.sub")}>{t("nav.groups")}</PageTitle>
      <form
        className="mb-6 flex gap-2"
        onSubmit={(e) => {
          e.preventDefault();
          if (name.trim()) wrap(fleet.saveGroup({ id: 0, name: name.trim(), energyPrice: null })).then(() => setName(""));
        }}
      >
        <input className={cx(compactInput, "flex-1")} value={name} onChange={(e) => setName(e.target.value)} placeholder={t("fleet.groups.ph")} aria-label={t("fleet.groups.new")} />
        <Button type="submit" variant="primary" disabled={!name.trim()}>
          <Plus /> {t("fleet.groups.new")}
        </Button>
      </form>
      {groups.length === 0 ? (
        <Empty>{t("fleet.groups.empty")}</Empty>
      ) : (
        <ul className="divide-y divide-line overflow-hidden rounded-2xl border border-line bg-surface shadow-[var(--highlight)]">
          {groups.map((g) => (
            <GroupRow key={g.id} g={g} count={members(g).length} mining={members(g).filter((d) => d.mining === "mining").length} onSave={(x) => wrap(fleet.saveGroup(x))} onStart={() => wrap(fleet.mining(members(g).map((d) => d.id), "start"))} onStop={() => setConfirm({ g, action: "stop" })} onDelete={() => setConfirm({ g, action: "delete" })} />
          ))}
        </ul>
      )}
      <ResultsModal results={results} onClose={() => setResults(null)} />
      <ConfirmDialog
        open={!!confirm}
        danger={confirm?.action === "delete"}
        title={confirm ? t(confirm.action === "stop" ? "fleet.groups.stopAll.title" : "fleet.groups.delete.title", { name: confirm.g.name, n: members(confirm.g).length }) : ""}
        text={confirm?.action === "stop" ? members(confirm.g).map((d) => d.name).join(", ") || "—" : t("fleet.groups.delete.text")}
        confirmLabel={confirm?.action === "stop" ? t("fleet.stop") : t("fleet.groups.delete")}
        cancelLabel={t("cancel")}
        onCancel={() => setConfirm(null)}
        onConfirm={() => {
          const c = confirm!;
          setConfirm(null);
          wrap(c.action === "stop" ? fleet.mining(members(c.g).map((d) => d.id), "stop") : fleet.deleteGroup(c.g.id));
        }}
      />
    </div>
  );
}

function GroupRow({ g, count, mining, onSave, onStart, onStop, onDelete }: { g: DeviceGroup; count: number; mining: number; onSave: (g: DeviceGroup) => void; onStart: () => void; onStop: () => void; onDelete: () => void }) {
  const t = useT();
  const [name, setName] = useState(g.name);
  const [price, setPrice] = useState(g.energyPrice?.toString() ?? "");
  const dirty = name !== g.name || price !== (g.energyPrice?.toString() ?? "");
  return (
    <li className="flex flex-wrap items-center gap-3 px-4 py-3">
      <div className="min-w-40 flex-1">
        <input className={cx(compactInput, "w-full font-medium")} value={name} onChange={(e) => setName(e.target.value)} aria-label={t("fleet.groups.name")} />
        <div className="mt-1 px-1 text-xs text-faint">{t("fleet.groups.count", { n: count, m: mining })}</div>
      </div>
      <label className="flex items-center gap-2 text-xs text-faint">
        {t("fleet.groups.price")}
        <input className={cx(compactInput, "w-24")} inputMode="decimal" value={price} onChange={(e) => setPrice(e.target.value)} placeholder="—" />
      </label>
      {dirty && (
        <Button size="sm" variant="primary" onClick={() => onSave({ ...g, name: name.trim(), energyPrice: price.trim() === "" ? null : Number(price.replace(",", ".")) })}>
          {t("save")}
        </Button>
      )}
      <Button size="sm" variant="go" disabled={!count} onClick={onStart} aria-label={t("fleet.groups.startAll", { name: g.name })}>
        <Play />
      </Button>
      <Button size="sm" disabled={!count} onClick={onStop} aria-label={t("fleet.groups.stopAll", { name: g.name })}>
        <Square />
      </Button>
      <Button size="sm" variant="ghost" onClick={onDelete} aria-label={t("fleet.groups.delete")}>
        <Trash2 />
      </Button>
    </li>
  );
}

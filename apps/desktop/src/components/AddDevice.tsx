import { HardDrive, Monitor, Radar, RefreshCw } from "lucide-react";
import { useState } from "react";
import { errorText, fleet, type DeviceKind, type FoundAgent } from "../lib/api";
import { translateError, useT } from "../lib/i18n";
import { useApp } from "../lib/store";
import { useLang } from "./fleet";
import { Button, Field, inputClass, Modal, Notice, Segmented } from "./ui";

/**
 * Add a device: a computer running erebor-agent (found on the network or by address, plus
 * the one-time code it shows), or an ASIC by address, directly or through a gateway agent.
 */
export function AddDevice({ open, onClose }: { open: boolean; onClose: () => void }) {
  const t = useT();
  const lang = useLang();
  const { fleet: list, refreshFleet, toast } = useApp();
  const [mode, setMode] = useState<"computer" | "asic">("computer");
  const [address, setAddress] = useState("");
  const [code, setCode] = useState("");
  const [kind, setKind] = useState<DeviceKind>("remote_pc");
  const [port, setPort] = useState("");
  const [name, setName] = useState("");
  const [via, setVia] = useState("");
  const [cidr, setCidr] = useState("");
  const [found, setFound] = useState<FoundAgent[] | null>(null);
  const [scan, setScan] = useState<{ host: string; port: number }[] | null>(null);
  const [busy, setBusy] = useState<"" | "add" | "discover" | "scan">("");
  const [error, setError] = useState("");
  const gateways = (list?.devices ?? []).filter((d) => d.kind !== "asic" && d.kind !== "local_pc");

  const run = async (what: typeof busy, f: () => Promise<unknown>) => {
    setBusy(what);
    setError("");
    try {
      await f();
    } catch (e) {
      setError(translateError(lang, errorText(e)));
    } finally {
      setBusy("");
    }
  };
  const done = async (msg: string) => {
    toast("info", msg);
    await refreshFleet();
    setAddress("");
    setCode("");
    setName("");
    onClose();
  };
  const add = () =>
    run("add", async () => {
      if (mode === "computer") {
        await fleet.addAgent(address.trim(), code.trim(), kind);
        await done(t("fleet.add.paired"));
      } else {
        await fleet.addAsic(address.trim(), port ? Number(port) : null, name.trim() || null, via || null);
        await done(t("fleet.add.asicAdded"));
      }
    });

  const canAdd = address.trim() !== "" && (mode === "asic" || code.replace(/[^a-z0-9]/gi, "").length === 12);
  return (
    <Modal
      open={open}
      onClose={onClose}
      closeLabel={t("close")}
      wide
      title={t("fleet.add.title")}
      actions={
        <>
          <Button variant="ghost" onClick={onClose}>
            {t("cancel")}
          </Button>
          <Button variant="primary" onClick={add} loading={busy === "add"} disabled={!canAdd}>
            {mode === "computer" ? t("fleet.add.pair") : t("fleet.add.addAsic")}
          </Button>
        </>
      }
    >
      <div className="space-y-5">
        <Segmented
          label={t("fleet.add.title")}
          value={mode}
          onChange={(m) => {
            setMode(m);
            setError("");
          }}
          options={[
            { value: "computer", label: t("fleet.add.computer"), icon: <Monitor /> },
            { value: "asic", label: t("fleet.add.asic"), icon: <HardDrive /> },
          ]}
        />
        {mode === "computer" ? (
          <>
            <div className="space-y-2 text-sm leading-relaxed">
              <p>{t("fleet.add.computer.how")}</p>
              <pre className="num overflow-x-auto rounded-xl bg-surface-2 px-3 py-2 text-xs text-fg">erebor-agent init{"\n"}sudo systemctl restart erebor-agent{"\n"}erebor-agent pair</pre>
            </div>
            <div>
              <div className="mb-2 flex items-center justify-between gap-2">
                <span className="text-sm font-medium text-muted">{t("fleet.add.found")}</span>
                <Button size="sm" variant="ghost" loading={busy === "discover"} onClick={() => run("discover", async () => setFound(await fleet.discover()))}>
                  <RefreshCw /> {t("fleet.add.search")}
                </Button>
              </div>
              {found && found.length === 0 && <p className="text-xs text-faint">{t("fleet.add.noneFound")}</p>}
              {found && found.length > 0 && (
                <ul className="divide-y divide-line rounded-xl border border-line">
                  {found.map((f) => (
                    <li key={f.found.id}>
                      <button
                        type="button"
                        disabled={f.paired}
                        onClick={() => setAddress(f.found.address)}
                        className="flex w-full items-center justify-between gap-3 px-3 py-2 text-left text-sm hover:bg-surface-2 disabled:opacity-50"
                      >
                        <span className="min-w-0">
                          <span className="block truncate font-medium text-fg">{f.found.name}</span>
                          <span className="num block text-xs text-faint">
                            {f.found.address} · v{f.found.version}
                          </span>
                        </span>
                        <span className="shrink-0 text-xs text-faint">{f.paired ? t("fleet.add.already") : f.found.pairing ? t("fleet.add.waiting") : ""}</span>
                      </button>
                    </li>
                  ))}
                </ul>
              )}
            </div>
            <div className="grid gap-4 sm:grid-cols-2">
              <Field label={t("fleet.add.address")} hint={t("fleet.add.address.hint")}>
                <input className={inputClass} value={address} onChange={(e) => setAddress(e.target.value)} placeholder="192.168.1.20" spellCheck={false} />
              </Field>
              <Field label={t("fleet.add.code")} hint={t("fleet.add.code.hint")}>
                <input className={`${inputClass} num uppercase`} value={code} onChange={(e) => setCode(e.target.value)} placeholder="XXXX-XXXX-XXXX" spellCheck={false} autoComplete="off" />
              </Field>
            </div>
            <Field label={t("fleet.add.kind")}>
              <select className={inputClass} value={kind} onChange={(e) => setKind(e.target.value as DeviceKind)}>
                {(["remote_pc", "server", "gpu_rig", "cpu_rig"] as const).map((k) => (
                  <option key={k} value={k}>
                    {t(`fleet.kind.${k}`)}
                  </option>
                ))}
              </select>
            </Field>
          </>
        ) : (
          <>
            <p className="text-sm leading-relaxed">{t("fleet.add.asic.how")}</p>
            <div className="grid gap-4 sm:grid-cols-[1fr_110px]">
              <Field label={t("fleet.add.ip")}>
                <input className={inputClass} value={address} onChange={(e) => setAddress(e.target.value)} placeholder="192.168.1.50" spellCheck={false} />
              </Field>
              <Field label={t("fleet.add.port")}>
                <input className={inputClass} value={port} onChange={(e) => setPort(e.target.value.replace(/\D/g, ""))} placeholder="4028" inputMode="numeric" />
              </Field>
            </div>
            <div className="grid gap-4 sm:grid-cols-2">
              <Field label={t("fleet.add.name")}>
                <input className={inputClass} value={name} onChange={(e) => setName(e.target.value)} placeholder={t("fleet.add.name.ph")} />
              </Field>
              <Field label={t("fleet.add.via")} hint={t("fleet.add.via.hint")}>
                <select className={inputClass} value={via} onChange={(e) => setVia(e.target.value)}>
                  <option value="">{t("fleet.add.direct")}</option>
                  {gateways.map((g) => (
                    <option key={g.id} value={g.id}>
                      {g.name}
                    </option>
                  ))}
                </select>
              </Field>
            </div>
            <div className="rounded-xl border border-line p-3">
              <div className="flex items-end gap-2">
                <div className="flex-1">
                  <Field label={t("fleet.add.scan")} hint={t("fleet.add.scan.hint")}>
                    <input className={inputClass} value={cidr} onChange={(e) => setCidr(e.target.value)} placeholder="192.168.1.0/24" spellCheck={false} />
                  </Field>
                </div>
              </div>
              <div className="mt-2 flex justify-end">
                <Button size="sm" loading={busy === "scan"} disabled={!cidr.includes("/")} onClick={() => run("scan", async () => setScan(await fleet.scan(cidr.trim(), via || null)))}>
                  <Radar /> {t("fleet.add.scanNow")}
                </Button>
              </div>
              {scan && (
                <ul className="mt-2 space-y-1 text-sm">
                  {scan.length === 0 && <li className="text-xs text-faint">{t("fleet.add.scanNone")}</li>}
                  {scan.map((s) => (
                    <li key={s.host} className="flex items-center justify-between gap-2">
                      <span className="num">
                        {s.host}:{s.port}
                      </span>
                      <Button size="sm" variant="ghost" onClick={() => (setAddress(s.host), setPort(String(s.port)))}>
                        {t("fleet.add.use")}
                      </Button>
                    </li>
                  ))}
                </ul>
              )}
            </div>
          </>
        )}
        {error && (
          <Notice tone="err">
            <span role="alert">{error}</span>
          </Notice>
        )}
      </div>
    </Modal>
  );
}

import { ArrowDown, ArrowUp, LayoutGrid, Play, Plus, RotateCcw, Rows3, Search, Square, Tag, X } from "lucide-react";
import { useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { AddDevice } from "../components/AddDevice";
import { ConnBadge, DeviceIcon, HistoryChart, kindLabel, MiningBadge, ResultsModal, SimBadge, useAgo, useFleetPolling, useLang, watts } from "../components/fleet";
import { PageTitle } from "../components/Layout";
import { Button, compactInput, ConfirmDialog, cx, Empty, Segmented } from "../components/ui";
import Groups from "./Groups";
import Profiles from "./Profiles";
import { errorText, fleet, type DeviceRow, type MiningProfile, type OpResult } from "../lib/api";
import { hashrate, money } from "../lib/format";
import { translateError, useT } from "../lib/i18n";
import { useApp } from "../lib/store";

type Filter = "all" | "online" | "offline" | "mining" | "problems";
type SortKey = "name" | "status" | "hashrate" | "temp" | "power" | "revenue" | "seen";
type GroupBy = "none" | "group" | "kind";
const ROW_H = 60;

const problem = (d: DeviceRow) => d.conn !== "online" || d.mining === "error" || !!d.error;

/**
 * Every device managed from here, this computer included: history, then a table with
 * search, filters, grouping and actions on a selection. Groups and profiles are tabs here.
 */
export default function Devices() {
  const t = useT();
  const lang = useLang();
  useFleetPolling();
  const { fleet: data, openDevice, refreshFleet, toast } = useApp();
  const [adding, setAdding] = useState(false);
  const [tab, setTab] = useState<"list" | "groups" | "profiles">("list");
  const [q, setQ] = useState("");
  const [filter, setFilter] = useState<Filter>("all");
  const [groupFilter, setGroupFilter] = useState<string>("");
  const [groupBy, setGroupBy] = useState<GroupBy>("none");
  const [view, setView] = useState<"table" | "cards">("table");
  const [sort, setSort] = useState<{ key: SortKey; desc: boolean }>({ key: "name", desc: false });
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [results, setResults] = useState<OpResult[] | null>(null);
  const [confirm, setConfirm] = useState<{ label: string; run: () => Promise<OpResult[] | void> } | null>(null);
  const [busy, setBusy] = useState(false);
  const [profiles, setProfiles] = useState<MiningProfile[]>([]);
  const [tagText, setTagText] = useState("");
  useEffect(() => {
    fleet.profiles().then(setProfiles).catch(() => {});
  }, []);

  const rows = data?.devices ?? [];
  const tags = useMemo(() => [...new Set(rows.flatMap((r) => r.tags))].sort(), [rows]);
  const shown = useMemo(() => {
    const needle = q.trim().toLowerCase();
    const list = rows.filter((r) => {
      if (needle && ![r.name, r.address, r.hardware, r.algo, r.coin, r.groupName, ...r.tags].some((s) => s?.toLowerCase().includes(needle))) return false;
      if (filter === "online" && r.conn !== "online") return false;
      if (filter === "offline" && r.conn === "online") return false;
      if (filter === "mining" && r.mining !== "mining") return false;
      if (filter === "problems" && !problem(r)) return false;
      if (groupFilter === "none" && r.group != null) return false;
      if (groupFilter.startsWith("g:") && String(r.group) !== groupFilter.slice(2)) return false;
      if (groupFilter.startsWith("t:") && !r.tags.includes(groupFilter.slice(2))) return false;
      return true;
    });
    const val = (r: DeviceRow): number | string => {
      switch (sort.key) {
        case "name":
          return r.name.toLowerCase();
        case "status":
          return `${r.conn === "online" ? 0 : 1}${r.mining}`;
        case "hashrate":
          return r.hashrate ?? -1;
        case "temp":
          return r.tempC ?? -1;
        case "power":
          return r.powerW ?? -1;
        case "revenue":
          return r.revenueDay ?? -1;
        case "seen":
          return r.lastSeen ?? 0;
      }
    };
    return list.sort((a, b) => {
      const [x, y] = [val(a), val(b)];
      const c = x < y ? -1 : x > y ? 1 : 0;
      return sort.desc ? -c : c;
    });
  }, [rows, q, filter, groupFilter, sort]);

  const sections = useMemo((): [string, DeviceRow[]][] => {
    if (groupBy === "none") return [["", shown]];
    const m = new Map<string, DeviceRow[]>();
    for (const r of shown) {
      const k = groupBy === "group" ? (r.groupName ?? t("fleet.ungrouped")) : kindLabel(t, r.kind);
      m.set(k, [...(m.get(k) ?? []), r]);
    }
    return [...m.entries()];
  }, [shown, groupBy, t]);

  const sel = rows.filter((r) => selected.has(r.id));
  const toggle = (id: string) => {
    const s = new Set(selected);
    if (s.has(id)) s.delete(id);
    else s.add(id);
    setSelected(s);
  };
  const allShown = shown.length > 0 && shown.every((r) => selected.has(r.id));

  const run = async (f: () => Promise<OpResult[] | void>) => {
    setBusy(true);
    try {
      const r = await f();
      if (r) setResults(r);
      await refreshFleet();
    } catch (e) {
      toast("error", translateError(lang, errorText(e)));
    } finally {
      setBusy(false);
    }
  };
  // Disruptive operations ask first and name the devices they touch.
  const ask = (label: string, f: () => Promise<OpResult[] | void>) => setConfirm({ label, run: f });
  const ids = sel.map((r) => r.id);

  if (!data) {
    return (
      <div>
        <PageTitle>{t("nav.devices")}</PageTitle>
        <div className="skeleton h-24 w-full" />
      </div>
    );
  }
  const o = data.overview;
  const m = o.money[0];
  const fmtMoney = (v: number | null | undefined) => (v == null || !m ? undefined : money(v, m.currency, lang));
  const temps = rows.flatMap((r) => (r.tempC != null && r.conn === "online" ? [r.tempC] : []));
  // The chart tabs show what the devices report now; the chart, what was recorded.
  const live: Record<string, string | undefined> = {
    power: o.powerDevices ? watts(o.powerW) : undefined,
    revenue: fmtMoney(m?.revenueDay),
    cost: fmtMoney(m?.costDay),
    net: fmtMoney(m?.netDay),
    temp: temps.length ? `${Math.round(Math.max(...temps))} °C` : undefined,
    ...Object.fromEntries(o.hashrate.map((x) => [`hash:${x.algo}`, hashrate(x.hashrate)])),
  };
  const counts = { all: o.total, online: o.online, offline: o.offline, mining: o.mining, problems: rows.filter(problem).length };

  return (
    <div>
      <PageTitle
        sub={t("fleet.sub")}
        action={
          <Button variant="primary" onClick={() => setAdding(true)}>
            <Plus /> {t("fleet.add.title")}
          </Button>
        }
      >
        {t("nav.devices")}
      </PageTitle>

      <div className="-mt-3 mb-6 flex flex-wrap items-center justify-between gap-3">
        <Segmented
          label={t("nav.devices")}
          value={tab}
          onChange={setTab}
          options={[
            { value: "list", label: t("fleet.tab.list") },
            { value: "groups", label: t("nav.groups") },
            { value: "profiles", label: t("nav.profiles") },
          ]}
        />
        {o.simulated > 0 && (
          <span className="inline-flex items-center gap-1.5 text-xs text-faint" title={t("fleet.simulated.notice", { n: o.simulated })}>
            <SimBadge /> {t("fleet.simulated.short", { n: o.simulated })}
          </span>
        )}
      </div>

      {tab === "groups" ? (
        <Groups />
      ) : tab === "profiles" ? (
        <Profiles />
      ) : (
        <>

      {o.money.length > 1 && <p className="mb-2 text-xs text-faint">{t("fleet.manyCurrencies", { list: o.money.map((x) => x.currency).join(", ") })}</p>}
      <HistoryChart id={null} live={live} />

      <div className="mt-8 flex flex-wrap items-center gap-2">
        <label className="relative min-w-48 flex-1">
          <Search className="pointer-events-none absolute top-1/2 left-3 size-4 -translate-y-1/2 text-faint" aria-hidden />
          <input className={cx(compactInput, "w-full pl-9")} value={q} onChange={(e) => setQ(e.target.value)} placeholder={t("fleet.search")} aria-label={t("fleet.search")} />
        </label>
        <select className={compactInput} value={groupFilter} onChange={(e) => setGroupFilter(e.target.value)} aria-label={t("fleet.filter.group")}>
          <option value="">{t("fleet.filter.allGroups")}</option>
          <option value="none">{t("fleet.ungrouped")}</option>
          {data.groups.map((g) => (
            <option key={g.id} value={`g:${g.id}`}>
              {g.name}
            </option>
          ))}
          {tags.map((tg) => (
            <option key={tg} value={`t:${tg}`}>
              #{tg}
            </option>
          ))}
        </select>
        <select className={compactInput} value={groupBy} onChange={(e) => setGroupBy(e.target.value as GroupBy)} aria-label={t("fleet.groupBy")}>
          <option value="none">{t("fleet.groupBy.none")}</option>
          <option value="group">{t("fleet.groupBy.group")}</option>
          <option value="kind">{t("fleet.groupBy.kind")}</option>
        </select>
        <Segmented
          size="sm"
          label={t("fleet.view")}
          value={view}
          onChange={setView}
          options={[
            { value: "table", label: "", icon: <Rows3 />, title: t("fleet.view.table") },
            { value: "cards", label: "", icon: <LayoutGrid />, title: t("fleet.view.cards") },
          ]}
        />
      </div>
      <div className="mt-3">
        <Segmented
          size="sm"
          label={t("fleet.filter")}
          value={filter}
          onChange={setFilter}
          options={(["all", "online", "offline", "mining", "problems"] as const).map((f) => ({ value: f, label: `${t(`fleet.filter.${f}`)} ${counts[f]}` }))}
        />
      </div>

      {sel.length > 0 && (
        <div className="sticky top-14 z-10 mt-4 flex flex-wrap items-center gap-2 rounded-2xl border border-line bg-surface px-3 py-2 shadow-[var(--highlight)]" role="toolbar" aria-label={t("fleet.selection")}>
          <span className="px-1 text-sm font-medium">{t("fleet.selected", { n: sel.length })}</span>
          <Button size="sm" variant="go" disabled={busy} onClick={() => run(() => fleet.mining(ids, "start"))}>
            <Play /> {t("fleet.start")}
          </Button>
          <Button size="sm" disabled={busy} onClick={() => ask(t("fleet.stop"), () => fleet.mining(ids, "stop"))}>
            <Square /> {t("fleet.stop")}
          </Button>
          <Button size="sm" disabled={busy} onClick={() => ask(t("fleet.restart"), () => fleet.mining(ids, "restart"))}>
            <RotateCcw /> {t("fleet.restart")}
          </Button>
          <select
            className={cx(compactInput, "h-8 text-xs")}
            value=""
            aria-label={t("fleet.applyProfile")}
            onChange={(e) => {
              const p = profiles.find((x) => String(x.id) === e.target.value);
              if (p) ask(t("fleet.applyProfileNamed", { name: p.name }), () => fleet.applyProfile(ids, p.id));
            }}
          >
            <option value="">{t("fleet.applyProfile")}</option>
            {profiles.map((p) => (
              <option key={p.id} value={p.id}>
                {p.name}
              </option>
            ))}
          </select>
          <select
            className={cx(compactInput, "h-8 text-xs")}
            value=""
            aria-label={t("fleet.assignGroup")}
            onChange={(e) => e.target.value && run(async () => void (await fleet.assignGroup(ids, e.target.value === "none" ? null : Number(e.target.value))))}
          >
            <option value="">{t("fleet.assignGroup")}</option>
            <option value="none">{t("fleet.ungrouped")}</option>
            {data.groups.map((g) => (
              <option key={g.id} value={g.id}>
                {g.name}
              </option>
            ))}
          </select>
          <form
            className="flex items-center gap-1"
            onSubmit={(e) => {
              e.preventDefault();
              const list = tagText.split(",").map((x) => x.trim()).filter(Boolean);
              if (list.length) run(async () => (await fleet.tag(ids, list), setTagText("")));
            }}
          >
            <input className={cx(compactInput, "h-8 w-28 text-xs")} value={tagText} onChange={(e) => setTagText(e.target.value)} placeholder={t("fleet.tags.ph")} aria-label={t("fleet.tags.add")} />
            <Button size="sm" variant="ghost" type="submit" aria-label={t("fleet.tags.add")}>
              <Tag />
            </Button>
          </form>
          <Button size="sm" variant="ghost" className="ml-auto" onClick={() => setSelected(new Set())} aria-label={t("fleet.clearSelection")}>
            <X />
          </Button>
        </div>
      )}

      <div className="mt-4">
        {rows.length === 0 ? (
          <Empty>{t("fleet.empty")}</Empty>
        ) : shown.length === 0 ? (
          <Empty>{t("fleet.noMatch")}</Empty>
        ) : view === "cards" ? (
          sections.map(([title, list]) => (
            <Section key={title} title={title}>
              <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
                {list.map((r) => (
                  <Card key={r.id} r={r} checked={selected.has(r.id)} onCheck={() => toggle(r.id)} onOpen={() => openDevice(r.id)} />
                ))}
              </div>
            </Section>
          ))
        ) : (
          <div className="overflow-hidden rounded-2xl border border-line bg-surface shadow-[var(--highlight)]">
            <div role="row" className="grid grid-cols-[36px_minmax(180px,2fr)_150px_110px_70px_80px_100px_90px] items-center gap-2 border-b border-line px-3 py-2 text-2xs font-semibold tracking-wide text-faint uppercase max-lg:grid-cols-[36px_minmax(160px,2fr)_140px_100px_80px]">
              <input type="checkbox" checked={allShown} aria-label={t("fleet.selectAll")} onChange={() => setSelected(allShown ? new Set() : new Set(shown.map((r) => r.id)))} className="size-4 accent-[var(--accent)]" />
              {(
                [
                  ["name", "fleet.col.name", ""],
                  ["status", "fleet.col.status", ""],
                  ["hashrate", "fleet.col.hashrate", "text-right"],
                  ["temp", "fleet.col.temp", "text-right max-lg:hidden"],
                  ["power", "fleet.col.power", "text-right"],
                  ["revenue", "fleet.col.revenue", "text-right max-lg:hidden"],
                  ["seen", "fleet.col.seen", "text-right max-lg:hidden"],
                ] as const
              ).map(([k, label, cls]) => (
                <button
                  key={k}
                  type="button"
                  className={cx("flex items-center gap-1 uppercase hover:text-fg", cls.includes("text-right") && "justify-end", cls)}
                  onClick={() => setSort({ key: k, desc: sort.key === k ? !sort.desc : k !== "name" })}
                  aria-sort={sort.key === k ? (sort.desc ? "descending" : "ascending") : undefined}
                >
                  {t(label)}
                  {sort.key === k && (sort.desc ? <ArrowDown className="size-3" /> : <ArrowUp className="size-3" />)}
                </button>
              ))}
            </div>
            {sections.map(([title, list]) => (
              <div key={title}>
                {title && <div className="border-b border-line bg-surface-2/60 px-4 py-1.5 text-xs font-semibold text-muted">{title} · {list.length}</div>}
                <Rows list={list} selected={selected} toggle={toggle} open={openDevice} />
              </div>
            ))}
          </div>
        )}
      </div>

        </>
      )}

      <AddDevice open={adding} onClose={() => setAdding(false)} />
      <ResultsModal results={results} onClose={() => setResults(null)} />
      <ConfirmDialog
        open={!!confirm}
        title={confirm ? t("fleet.confirm.title", { action: confirm.label, n: sel.length }) : ""}
        text={<span className="block max-h-40 overflow-y-auto">{sel.map((r) => r.name).join(", ")}</span>}
        confirmLabel={confirm?.label ?? ""}
        cancelLabel={t("cancel")}
        onCancel={() => setConfirm(null)}
        onConfirm={() => {
          const c = confirm!;
          setConfirm(null);
          run(c.run);
        }}
      />
    </div>
  );
}

function Section({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section className="mb-5">
      {title && <h3 className="mb-2 px-1 text-sm font-semibold text-muted">{title}</h3>}
      {children}
    </section>
  );
}

/**
 * Table rows. Long lists render only what is on screen (fixed row height), so hundreds of
 * devices stay smooth.
 */
function Rows({ list, selected, toggle, open }: { list: DeviceRow[]; selected: Set<string>; toggle: (id: string) => void; open: (id: string) => void }) {
  const ref = useRef<HTMLDivElement>(null);
  const [top, setTop] = useState(0);
  const virtual = list.length > 120;
  const height = Math.min(list.length * ROW_H, 640);
  const first = virtual ? Math.max(0, Math.floor(top / ROW_H) - 5) : 0;
  const last = virtual ? Math.min(list.length, first + Math.ceil(height / ROW_H) + 10) : list.length;
  return (
    <div ref={ref} onScroll={(e) => setTop(e.currentTarget.scrollTop)} style={virtual ? { height, overflowY: "auto" } : undefined}>
      <div style={virtual ? { height: list.length * ROW_H, position: "relative" } : undefined}>
        {list.slice(first, last).map((r, i) => (
          <div key={r.id} style={virtual ? { position: "absolute", top: (first + i) * ROW_H, left: 0, right: 0 } : undefined}>
            <TableRow r={r} checked={selected.has(r.id)} onCheck={() => toggle(r.id)} onOpen={() => open(r.id)} />
          </div>
        ))}
      </div>
    </div>
  );
}

function TableRow({ r, checked, onCheck, onOpen }: { r: DeviceRow; checked: boolean; onCheck: () => void; onOpen: () => void }) {
  const t = useT();
  const lang = useLang();
  const ago = useAgo();
  return (
    <div
      role="row"
      style={{ height: ROW_H }}
      className={cx(
        "grid cursor-pointer grid-cols-[36px_minmax(180px,2fr)_150px_110px_70px_80px_100px_90px] items-center gap-2 border-b border-line px-3 text-sm transition-colors last:border-0 hover:bg-surface-2/70 max-lg:grid-cols-[36px_minmax(160px,2fr)_140px_100px_80px]",
        checked && "bg-accent-soft/60",
      )}
      onClick={onOpen}
    >
      <input type="checkbox" checked={checked} onChange={onCheck} onClick={(e) => e.stopPropagation()} aria-label={t("fleet.select", { name: r.name })} className="size-4 accent-[var(--accent)]" />
      <div className="flex min-w-0 items-center gap-2.5">
        <DeviceIcon row={r} className="size-4" />
        <div className="min-w-0">
          <div className="flex items-center gap-1.5">
            <button type="button" className="truncate text-left font-medium text-fg hover:underline" onClick={(e) => (e.stopPropagation(), onOpen())}>
              {r.name}
            </button>
            {r.simulated && <SimBadge />}
          </div>
          <div className="truncate text-xs text-faint">
            {[kindLabel(t, r.kind), r.coin ?? r.algo, r.groupName, ...r.tags.map((x) => `#${x}`)].filter(Boolean).join(" · ")}
          </div>
        </div>
      </div>
      <div className="flex flex-wrap gap-1" title={r.error ? translateError(lang, r.error) : undefined}>
        <ConnBadge conn={r.conn} />
        {r.conn === "online" && <MiningBadge mining={r.mining} />}
      </div>
      <div className="num text-right">
        <div>{r.hashrate != null ? hashrate(r.hashrate) : "—"}</div>
        {r.algo && <div className="text-2xs text-faint">{r.algo}</div>}
      </div>
      <div className={cx("num text-right max-lg:hidden", r.tempC != null && r.tempC >= 85 && "text-danger")}>{r.tempC != null ? `${Math.round(r.tempC)}°` : "—"}</div>
      <div className="num text-right" title={r.powerW != null ? t(r.powerMeasured ? "fleet.power.measured" : "fleet.power.estimated") : undefined}>
        {r.powerW != null ? `${Math.round(r.powerW)} W` : "—"}
        {r.powerW != null && !r.powerMeasured && <span className="text-faint">*</span>}
      </div>
      <div className="num text-right max-lg:hidden">{r.revenueDay != null ? money(r.revenueDay, r.currency, lang) : "—"}</div>
      <div className="num text-right text-xs text-faint max-lg:hidden">{ago(r.lastSeen)}</div>
    </div>
  );
}

function Card({ r, checked, onCheck, onOpen }: { r: DeviceRow; checked: boolean; onCheck: () => void; onOpen: () => void }) {
  const t = useT();
  return (
    <div className={cx("panel cursor-pointer p-4 transition-colors hover:bg-surface-2/60", checked && "ring-2 ring-accent/40")} onClick={onOpen}>
      <div className="flex items-start justify-between gap-2">
        <div className="flex min-w-0 items-center gap-2">
          <DeviceIcon row={r} className="size-4" />
          <span className="truncate font-medium">{r.name}</span>
        </div>
        <input type="checkbox" checked={checked} onChange={onCheck} onClick={(e) => e.stopPropagation()} aria-label={t("fleet.select", { name: r.name })} className="size-4 accent-[var(--accent)]" />
      </div>
      <div className="mt-2 flex flex-wrap gap-1">
        <ConnBadge conn={r.conn} />
        {r.conn === "online" && <MiningBadge mining={r.mining} />}
        {r.simulated && <SimBadge />}
      </div>
      <dl className="mt-3 grid grid-cols-3 gap-2 text-xs">
        <div>
          <dt className="text-faint">{t("fleet.col.hashrate")}</dt>
          <dd className="num">{r.hashrate != null ? hashrate(r.hashrate) : "—"}</dd>
        </div>
        <div>
          <dt className="text-faint">{t("fleet.col.temp")}</dt>
          <dd className="num">{r.tempC != null ? `${Math.round(r.tempC)} °C` : "—"}</dd>
        </div>
        <div>
          <dt className="text-faint">{t("fleet.col.power")}</dt>
          <dd className="num">{r.powerW != null ? `${Math.round(r.powerW)} W` : "—"}</dd>
        </div>
      </dl>
    </div>
  );
}

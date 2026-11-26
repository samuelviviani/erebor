// Simulated device management for tests and the browser-only preview. Every device is
// flagged `simulated`; nothing here talks to real hardware.
import type { DeviceGroup, DeviceRow, FleetEvent, FleetSettings, MiningProfile, OpResult } from "../lib/api";

const cap = (supported: boolean, reason: string | null = null) => ({ supported, reason: supported ? null : reason });

function row(p: Partial<DeviceRow> & Pick<DeviceRow, "id" | "name" | "kind">): DeviceRow {
  const asic = p.kind === "asic";
  return {
    address: "127.0.0.1",
    group: null,
    groupName: null,
    tags: [],
    conn: "online",
    mining: "idle",
    error: null,
    simulated: true,
    hardware: asic ? "Antminer S19 (Erebor simulator)" : "Demo CPU",
    os: asic ? null : "Linux 6.8",
    version: asic ? "BMMiner 2.0.0" : "0.1.7",
    algo: asic ? "sha256d" : "rx/0",
    coin: asic ? null : "XMR",
    pool: "stratum+tcp://sim-pool.invalid:3333",
    hashrate: null,
    tempC: 55,
    powerW: asic ? 3250 : 140,
    powerMeasured: asic,
    revenueDay: null,
    costDay: null,
    netDay: null,
    currency: "EUR",
    accepted: 120,
    rejected: 1,
    lastSeen: Math.floor(Date.now() / 1000),
    gateway: null,
    actions: {
      start: cap(!asic, "asic_no_stop_command"),
      stop: cap(!asic, "asic_no_stop_command"),
      restart: cap(true),
      configure: cap(!asic, "asic_configure_via_pool"),
      setPool: cap(true),
      remove: cap(p.kind !== "local_pc", "cannot_remove_local"),
      rename: cap(true),
    },
    ...p,
  };
}

export function mockFleet(withDevices: boolean) {
  let seq = 1;
  const events: FleetEvent[] = [];
  const ev = (deviceId: string | null, level: FleetEvent["level"], code: string, detail: string | null = null) =>
    events.unshift({ id: seq++, ts: Math.floor(Date.now() / 1000), deviceId, deviceName: devices.find((d) => d.id === deviceId)?.name ?? null, level, code, detail });
  let devices: DeviceRow[] = [row({ id: "local", name: "This PC", kind: "local_pc", simulated: false, conn: "offline" })];
  if (withDevices) {
    devices.push(
      row({ id: "agent:rig1", name: "sim-rig-1", kind: "gpu_rig", mining: "mining", hashrate: 2430, revenueDay: 0.31, costDay: 0.84, netDay: -0.53 }),
      row({ id: "agent:pc2", name: "sim-pc-2", kind: "remote_pc" }),
      row({ id: "agent:off", name: "sim-offline", kind: "server", conn: "offline", tempC: null, powerW: null, error: "unreachable" }),
      row({ id: "asic:127.0.0.1:17524", name: "sim-s19-hot", kind: "asic", mining: "mining", hashrate: 95e12, tempC: 96, revenueDay: 4.1 }),
    );
  }
  const groups: DeviceGroup[] = [];
  let profiles: MiningProfile[] = [];
  let settings: FleetSettings = { currency: "EUR", energyPrice: null, maxTempC: 85, asicMaxTempC: 90, hashrateDropPercent: 30, powerRisePercent: 50, rejectedPercent: 5, cooldownMinutes: 30, retentionDays: 30, discovery: true };
  const results = (ids: string[], f: (d: DeviceRow) => string | null): OpResult[] =>
    ids.map((id) => {
      const d = devices.find((x) => x.id === id)!;
      const error = d.conn !== "online" ? "unreachable" : f(d);
      return { id, name: d.name, state: error ? "failed" : "completed", error };
    });
  const overview = () => {
    const algos = new Map<string, { hashrate: number; devices: number }>();
    for (const d of devices)
      if (d.algo && d.hashrate != null) {
        const a = algos.get(d.algo) ?? { hashrate: 0, devices: 0 };
        algos.set(d.algo, { hashrate: a.hashrate + d.hashrate, devices: a.devices + 1 });
      }
    const sum = (k: "revenueDay" | "costDay") => devices.reduce((s, d) => s + (d[k] ?? 0), 0);
    const power = devices.filter((d) => d.powerW != null && d.conn === "online");
    return {
      total: devices.length,
      online: devices.filter((d) => d.conn === "online").length,
      offline: devices.filter((d) => d.conn !== "online").length,
      mining: devices.filter((d) => d.mining === "mining").length,
      simulated: devices.filter((d) => d.simulated).length,
      powerW: power.reduce((s, d) => s + d.powerW!, 0),
      powerMeasuredW: power.filter((d) => d.powerMeasured).reduce((s, d) => s + d.powerW!, 0),
      powerDevices: power.length,
      hashrate: [...algos].map(([algo, v]) => ({ algo, ...v })),
      money: [{ currency: "EUR", revenueDay: sum("revenueDay"), costDay: sum("costDay"), netDay: sum("revenueDay") - sum("costDay") }],
    };
  };

  return (cmd: string, a: Record<string, any>): unknown => {
    switch (cmd) {
      case "fleet_list":
        devices = devices.map((d) => ({ ...d, groupName: groups.find((g) => g.id === d.group)?.name ?? null }));
        return { overview: overview(), devices, groups, identity: "0000:5157:0000:0000" };
      case "fleet_refresh":
        return null;
      case "fleet_discover":
        return [{ found: { address: "192.168.1.30:7420", id: "abc", name: "sim-found", version: "0.1.7", fingerprint: "", pairing: true }, paired: false }];
      case "fleet_add_agent": {
        if (String(a.code).replace(/[^a-z0-9]/gi, "").length !== 12) throw "pairing_code_invalid";
        const d = row({ id: `agent:${seq}`, name: `sim-agent-${seq}`, kind: a.kind ?? "remote_pc", address: a.address });
        devices.push(d);
        ev(d.id, "info", "device_added");
        return d;
      }
      case "fleet_add_asic": {
        const d = row({ id: `asic:${a.host}:${a.port ?? 4028}`, name: a.name || "Antminer S19 (Erebor simulator)", kind: "asic", address: `${a.host}:${a.port ?? 4028}`, gateway: a.via });
        if (devices.some((x) => x.id === d.id)) throw "device_exists";
        devices.push(d);
        return d;
      }
      case "fleet_scan":
        return [{ host: "192.168.1.50", port: 4028 }];
      case "fleet_remove":
        if (a.id === "local") throw "cannot_remove_local";
        devices = devices.filter((d) => d.id !== a.id);
        return null;
      case "fleet_rename":
        devices = devices.map((d) => (d.id === a.id ? { ...d, name: a.name } : d));
        return null;
      case "fleet_set_kind":
        devices = devices.map((d) => (d.id === a.id ? { ...d, kind: a.kind } : d));
        return null;
      case "fleet_set_price":
        return null;
      case "fleet_mining": {
        const r = results(a.ids, (d) => (d.kind === "asic" && a.action !== "restart" ? "asic_no_stop_command" : null));
        for (const x of r)
          if (x.state === "completed") devices = devices.map((d) => (d.id === x.id ? { ...d, mining: a.action === "stop" ? "idle" : "mining", hashrate: a.action === "stop" ? null : (d.hashrate ?? 2400) } : d));
        r.forEach((x) => ev(x.id, x.error ? "warning" : "info", `command_${a.action}`, x.error));
        return r;
      }
      case "fleet_apply":
        return results(a.ids, () => null);
      case "fleet_assign_group":
        devices = devices.map((d) => (a.ids.includes(d.id) ? { ...d, group: a.group } : d));
        return a.ids.length;
      case "fleet_tag":
        devices = devices.map((d) => (a.ids.includes(d.id) ? { ...d, tags: a.remove ? d.tags.filter((t) => !a.tags.includes(t)) : [...new Set([...d.tags, ...a.tags])] } : d));
        return null;
      case "fleet_save_group": {
        const g = a.group as DeviceGroup;
        if (groups.some((x) => x.name.toLowerCase() === g.name.toLowerCase() && x.id !== g.id)) throw "group_exists";
        if (g.id) Object.assign(groups.find((x) => x.id === g.id)!, g);
        else groups.push({ ...g, id: seq++ });
        return g;
      }
      case "fleet_delete_group":
        groups.splice(groups.findIndex((g) => g.id === a.id), 1);
        return null;
      case "fleet_profiles":
        return profiles;
      case "fleet_save_profile": {
        const p = a.profile as MiningProfile;
        profiles = p.id ? profiles.map((x) => (x.id === p.id ? p : x)) : [...profiles, { ...p, id: seq++ }];
        return p;
      }
      case "fleet_delete_profile":
        profiles = profiles.filter((p) => p.id !== a.id);
        return null;
      case "fleet_config":
        throw "device_offline";
      case "fleet_set_config":
        return null;
      case "fleet_query":
        return a.cmd === "logs" ? ["[sim] miner started"] : a.cmd === "coins" ? [] : null;
      case "fleet_history":
        return { bucketSecs: 900, points: [], hashrate: {} };
      case "fleet_events":
        return events.filter((e) => e.id > (a.sinceId ?? 0)).slice(0, a.limit ?? 200);
      case "fleet_settings":
        return settings;
      case "fleet_set_settings":
        settings = a.settings;
        return settings;
      case "fleet_export":
        return { erebor: "controller-export", version: 1, devices, groups, profiles, settings };
      case "fleet_import":
        return { added: 0 };
      default:
        throw `mock: unknown command ${cmd}`;
    }
  };
}

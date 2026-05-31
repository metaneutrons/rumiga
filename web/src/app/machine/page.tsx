'use client';

import { useEffect, useState } from 'react';
import {
  getMachineConfig,
  updateMachineConfig,
  startMachine,
  stopMachine,
  getMachineStatus,
  getSupportBundle,
  type MachineConfig,
  type MachineStatus,
  type AmigaModel,
  type ScalingMode,
  type ViewportMode,
  type ViewportPreset,
  type FloppySpeedPercent,
  type HdfWritePolicy,
  type NetworkBackend,
} from '@/lib/api';

const DEFAULT_VIEWPORT: MachineConfig['display']['viewport'] = {
  mode: 'Auto',
  preset: 'AutoCenter',
  x: 0,
  y: 0,
  width: 754,
  height: 288,
  vertical_stretch: true,
};
const DEFAULT_FLOPPY_SPEED_PERCENT: FloppySpeedPercent = 100;
const FLOPPY_SPEED_OPTIONS: FloppySpeedPercent[] = [100, 200, 400, 800, 0];
const HDF_WRITE_POLICIES: Array<{ value: HdfWritePolicy; label: string }> = [
  { value: 'ReadOnly', label: 'Read-only session' },
  { value: 'Writeback', label: 'Writeback on exit' },
];
const NETWORK_BACKENDS: Array<{ value: NetworkBackend; label: string }> = [
  { value: 'Disabled', label: 'Disabled' },
  { value: 'Slirp', label: 'SLIRP / NAT' },
];
const DEFAULT_NETWORK: MachineConfig['network'] = {
  device: 'A2065',
  backend: 'Disabled',
  mac_address: '00:80:10:4d:49:47',
};
type ViewportChoice = ViewportPreset | 'Manual';
const VIEWPORT_CHOICES: Array<{ value: ViewportChoice; label: string }> = [
  { value: 'AutoCenter', label: 'Auto center' },
  { value: 'VisibleArea', label: 'Visible area' },
  { value: 'NativeFullBorder', label: 'Native full border' },
  { value: 'Overscan', label: 'Overscan' },
  { value: 'Manual', label: 'Manual crop' },
];

function isFloppySpeedPercent(value: unknown): value is FloppySpeedPercent {
  return (
    typeof value === 'number' &&
    (value === 0 || value === 100 || value === 200 || value === 400 || value === 800)
  );
}

function viewportModeForChoice(choice: ViewportChoice): ViewportMode {
  if (choice === 'Manual') return 'Manual';
  return choice === 'NativeFullBorder' || choice === 'Overscan' ? 'Raw' : 'Auto';
}

function viewportChoiceForConfig(viewport: MachineConfig['display']['viewport']): ViewportChoice {
  return viewport.mode === 'Manual' ? 'Manual' : viewport.preset;
}

function networkAutoconfigLabel(status: MachineStatus['network']): string {
  if (status.a2065_configured) return status.a2065_base_address ?? 'configured';
  if (status.a2065_present) return 'waiting';
  return 'absent';
}

function normalizeConfig(config: MachineConfig): MachineConfig {
  return {
    ...config,
    floppy_speed_percent: isFloppySpeedPercent(config.floppy_speed_percent)
      ? config.floppy_speed_percent
      : DEFAULT_FLOPPY_SPEED_PERCENT,
    hdf_path: config.hdf_path ?? null,
    hdf_write_policy: config.hdf_write_policy === 'Writeback' ? 'Writeback' : 'ReadOnly',
    network: {
      ...DEFAULT_NETWORK,
      ...config.network,
    },
    display: {
      ...config.display,
      viewport: {
        ...DEFAULT_VIEWPORT,
        ...config.display.viewport,
      },
    },
  };
}

export default function MachinePage() {
  const [config, setConfig] = useState<MachineConfig | null>(null);
  const [status, setStatus] = useState<MachineStatus | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [supportBusy, setSupportBusy] = useState(false);

  useEffect(() => {
    getMachineConfig()
      .then((r) => {
        if (r.success && r.data) setConfig(normalizeConfig(r.data));
        else setError(r.error ?? 'Failed to load config');
      })
      .catch((e: Error) => setError(e.message));
    getMachineStatus()
      .then((r) => {
        if (r.success && r.data) setStatus(r.data);
      })
      .catch(() => {});
  }, []);

  async function handleSave(e: React.FormEvent) {
    e.preventDefault();
    if (!config) return;
    setSaving(true);
    setError(null);
    try {
      const r = await updateMachineConfig(config);
      if (!r.success) setError(r.error ?? 'Save failed');
    } catch (err: unknown) {
      setError(err instanceof Error ? err.message : 'Save failed');
    } finally {
      setSaving(false);
    }
  }

  async function handleStart() {
    try {
      const r = await startMachine();
      if (!r.success) setError(r.error ?? 'Start failed');
      else {
        const s = await getMachineStatus();
        if (s.success && s.data) setStatus(s.data);
      }
    } catch (err: unknown) {
      setError(err instanceof Error ? err.message : 'Start failed');
    }
  }

  async function handleStop() {
    try {
      const r = await stopMachine();
      if (!r.success) setError(r.error ?? 'Stop failed');
      else {
        const s = await getMachineStatus();
        if (s.success && s.data) setStatus(s.data);
      }
    } catch (err: unknown) {
      setError(err instanceof Error ? err.message : 'Stop failed');
    }
  }

  async function handleSupportBundle() {
    setSupportBusy(true);
    setError(null);
    try {
      const r = await getSupportBundle();
      if (!r.success || !r.data) {
        setError(r.error ?? 'Support bundle failed');
        return;
      }
      const blob = new Blob([JSON.stringify(r.data, null, 2)], {
        type: 'application/json',
      });
      const url = URL.createObjectURL(blob);
      const link = document.createElement('a');
      link.href = url;
      link.download = `rumiga-support-${Date.now()}.json`;
      link.click();
      URL.revokeObjectURL(url);
    } catch (err: unknown) {
      setError(err instanceof Error ? err.message : 'Support bundle failed');
    } finally {
      setSupportBusy(false);
    }
  }

  if (!config) {
    return <p className="text-zinc-400">{error ?? 'Loading…'}</p>;
  }

  const viewport = config.display.viewport ?? DEFAULT_VIEWPORT;
  const networkStatus = status?.network;

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between">
        <h1 className="text-2xl font-bold">Machine</h1>
        <div className="flex gap-2">
          <button
            onClick={handleSupportBundle}
            disabled={supportBusy}
            className="rounded bg-zinc-800 px-3 py-1.5 text-sm font-medium text-zinc-100 hover:bg-zinc-700 disabled:opacity-50"
          >
            {supportBusy ? 'Bundling' : 'Support JSON'}
          </button>
          {status?.running ? (
            <button
              onClick={handleStop}
              className="rounded bg-red-700 px-3 py-1.5 text-sm font-medium hover:bg-red-600"
            >
              Stop
            </button>
          ) : (
            <button
              onClick={handleStart}
              className="rounded bg-green-700 px-3 py-1.5 text-sm font-medium hover:bg-green-600"
            >
              Start
            </button>
          )}
        </div>
      </div>

      {error && <p className="text-red-400">{error}</p>}

      <form onSubmit={handleSave} className="space-y-4 max-w-lg">
        <fieldset className="space-y-3">
          <legend className="text-lg font-semibold">Hardware</legend>

          <label className="block">
            <span className="text-sm text-zinc-400">Model</span>
            <select
              value={config.model}
              onChange={(e) => setConfig({ ...config, model: e.target.value as AmigaModel })}
              className="mt-1 block w-full rounded border border-zinc-700 bg-zinc-800 px-3 py-2 text-sm"
            >
              <option value="A500">A500</option>
              <option value="A500Plus">A500+</option>
              <option value="A1200">A1200</option>
            </select>
          </label>

          <div className="grid grid-cols-3 gap-3">
            <label className="block">
              <span className="text-sm text-zinc-400">Chip RAM (KB)</span>
              <input
                type="number"
                value={config.chip_ram_kb}
                onChange={(e) => setConfig({ ...config, chip_ram_kb: Number(e.target.value) })}
                className="mt-1 block w-full rounded border border-zinc-700 bg-zinc-800 px-3 py-2 text-sm"
              />
            </label>
            <label className="block">
              <span className="text-sm text-zinc-400">Slow RAM (KB)</span>
              <input
                type="number"
                value={config.slow_ram_kb}
                onChange={(e) => setConfig({ ...config, slow_ram_kb: Number(e.target.value) })}
                className="mt-1 block w-full rounded border border-zinc-700 bg-zinc-800 px-3 py-2 text-sm"
              />
            </label>
            <label className="block">
              <span className="text-sm text-zinc-400">Fast RAM (KB)</span>
              <input
                type="number"
                value={config.fast_ram_kb}
                onChange={(e) => setConfig({ ...config, fast_ram_kb: Number(e.target.value) })}
                className="mt-1 block w-full rounded border border-zinc-700 bg-zinc-800 px-3 py-2 text-sm"
              />
            </label>
          </div>

          <label className="block">
            <span className="text-sm text-zinc-400">ROM File</span>
            <input
              type="text"
              value={config.rom_file}
              onChange={(e) => setConfig({ ...config, rom_file: e.target.value })}
              className="mt-1 block w-full rounded border border-zinc-700 bg-zinc-800 px-3 py-2 text-sm"
            />
          </label>
        </fieldset>

        <fieldset className="space-y-3">
          <legend className="text-lg font-semibold">Floppy Drives</legend>
          <label className="block">
            <span className="text-sm text-zinc-400">Drive speed</span>
            <select
              value={config.floppy_speed_percent}
              onChange={(e) =>
                setConfig({
                  ...config,
                  floppy_speed_percent: Number(e.target.value) as FloppySpeedPercent,
                })
              }
              className="mt-1 block w-full rounded border border-zinc-700 bg-zinc-800 px-3 py-2 text-sm"
            >
              {FLOPPY_SPEED_OPTIONS.map((speed) => (
                <option key={speed} value={speed}>
                  {speed === 0 ? 'Turbo' : `${speed}%${speed === 100 ? ' compatible' : ''}`}
                </option>
              ))}
            </select>
          </label>
          {config.floppy.map((disk, i) => (
            <label key={i} className="block">
              <span className="text-sm text-zinc-400">DF{i}:</span>
              <input
                type="text"
                value={disk ?? ''}
                onChange={(e) => {
                  const floppy = [...config.floppy] as MachineConfig['floppy'];
                  floppy[i] = e.target.value || null;
                  setConfig({ ...config, floppy });
                }}
                placeholder="(empty)"
                className="mt-1 block w-full rounded border border-zinc-700 bg-zinc-800 px-3 py-2 text-sm"
              />
            </label>
          ))}
        </fieldset>

        <fieldset className="space-y-3">
          <legend className="text-lg font-semibold">Hard Drive</legend>
          <label className="block">
            <span className="text-sm text-zinc-400">Gayle IDE HDF</span>
            <input
              type="text"
              value={config.hdf_path ?? ''}
              onChange={(e) => setConfig({ ...config, hdf_path: e.target.value || null })}
              placeholder="(empty)"
              className="mt-1 block w-full rounded border border-zinc-700 bg-zinc-800 px-3 py-2 text-sm"
            />
          </label>
          <label className="block">
            <span className="text-sm text-zinc-400">Write policy</span>
            <select
              value={config.hdf_write_policy}
              onChange={(e) =>
                setConfig({ ...config, hdf_write_policy: e.target.value as HdfWritePolicy })
              }
              className="mt-1 block w-full rounded border border-zinc-700 bg-zinc-800 px-3 py-2 text-sm"
            >
              {HDF_WRITE_POLICIES.map((policy) => (
                <option key={policy.value} value={policy.value}>
                  {policy.label}
                </option>
              ))}
            </select>
          </label>
        </fieldset>

        <fieldset className="space-y-3">
          <legend className="text-lg font-semibold">Network</legend>
          <label className="block">
            <span className="text-sm text-zinc-400">A2065 backend</span>
            <select
              value={config.network.backend}
              onChange={(e) =>
                setConfig({
                  ...config,
                  network: { ...config.network, backend: e.target.value as NetworkBackend },
                })
              }
              className="mt-1 block w-full rounded border border-zinc-700 bg-zinc-800 px-3 py-2 text-sm"
            >
              {NETWORK_BACKENDS.map((backend) => (
                <option key={backend.value} value={backend.value}>
                  {backend.label}
                </option>
              ))}
            </select>
          </label>
          <label className="block">
            <span className="text-sm text-zinc-400">MAC address</span>
            <input
              type="text"
              value={config.network.mac_address}
              onChange={(e) =>
                setConfig({
                  ...config,
                  network: { ...config.network, mac_address: e.target.value },
                })
              }
              className="mt-1 block w-full rounded border border-zinc-700 bg-zinc-800 px-3 py-2 font-mono text-sm"
            />
          </label>
          {networkStatus && (
            <div className="grid gap-2 rounded border border-zinc-800 bg-zinc-950/40 p-3 text-sm sm:grid-cols-2">
              <div>
                <span className="block text-xs uppercase text-zinc-500">Link</span>
                <span className={networkStatus.link_up ? 'font-medium text-emerald-400' : 'font-medium text-zinc-400'}>
                  {networkStatus.link_up ? 'Up' : 'Down'}
                </span>
              </div>
              <div>
                <span className="block text-xs uppercase text-zinc-500">Autoconfig</span>
                <span className="font-mono text-zinc-200">
                  {networkAutoconfigLabel(networkStatus)}
                </span>
              </div>
              <div>
                <span className="block text-xs uppercase text-zinc-500">TX packets</span>
                <span className="font-mono text-zinc-200">{networkStatus.counters.tx_packets}</span>
              </div>
              <div>
                <span className="block text-xs uppercase text-zinc-500">RX / dropped</span>
                <span className="font-mono text-zinc-200">
                  {networkStatus.counters.rx_packets} / {networkStatus.counters.dropped_packets}
                </span>
              </div>
            </div>
          )}
        </fieldset>

        <fieldset className="space-y-3">
          <legend className="text-lg font-semibold">Display</legend>
          <label className="block">
            <span className="text-sm text-zinc-400">Scaling</span>
            <select
              value={config.display.scaling}
              onChange={(e) =>
                setConfig({
                  ...config,
                  display: { ...config.display, scaling: e.target.value as ScalingMode },
                })
              }
              className="mt-1 block w-full rounded border border-zinc-700 bg-zinc-800 px-3 py-2 text-sm"
            >
              <option value="Integer">Integer</option>
              <option value="AspectFit">Aspect Fit</option>
              <option value="Stretch">Stretch</option>
            </select>
          </label>
          <label className="block">
            <span className="text-sm text-zinc-400">Viewport preset</span>
            <select
              value={viewportChoiceForConfig(viewport)}
              onChange={(e) => {
                const choice = e.target.value as ViewportChoice;
                setConfig({
                  ...config,
                  display: {
                    ...config.display,
                    viewport: {
                      ...viewport,
                      mode: viewportModeForChoice(choice),
                      preset: choice === 'Manual' ? viewport.preset : choice,
                    },
                  },
                });
              }}
              className="mt-1 block w-full rounded border border-zinc-700 bg-zinc-800 px-3 py-2 text-sm"
            >
              {VIEWPORT_CHOICES.map((choice) => (
                <option key={choice.value} value={choice.value}>
                  {choice.label}
                </option>
              ))}
            </select>
          </label>
          <label className="flex items-center gap-2">
            <input
              type="checkbox"
              checked={viewport.vertical_stretch}
              onChange={(e) =>
                setConfig({
                  ...config,
                  display: {
                    ...config.display,
                    viewport: { ...viewport, vertical_stretch: e.target.checked },
                  },
                })
              }
              className="rounded border-zinc-700"
            />
            <span className="text-sm">PAL line-double</span>
          </label>
          {viewport.mode === 'Manual' && (
            <div className="grid grid-cols-2 gap-3">
              <label className="block">
                <span className="text-sm text-zinc-400">X</span>
                <input
                  type="number"
                  value={viewport.x}
                  onChange={(e) =>
                    setConfig({
                      ...config,
                      display: {
                        ...config.display,
                        viewport: { ...viewport, x: Number(e.target.value) },
                      },
                    })
                  }
                  className="mt-1 block w-full rounded border border-zinc-700 bg-zinc-800 px-3 py-2 text-sm"
                />
              </label>
              <label className="block">
                <span className="text-sm text-zinc-400">Y</span>
                <input
                  type="number"
                  value={viewport.y}
                  onChange={(e) =>
                    setConfig({
                      ...config,
                      display: {
                        ...config.display,
                        viewport: { ...viewport, y: Number(e.target.value) },
                      },
                    })
                  }
                  className="mt-1 block w-full rounded border border-zinc-700 bg-zinc-800 px-3 py-2 text-sm"
                />
              </label>
              <label className="block">
                <span className="text-sm text-zinc-400">Width</span>
                <input
                  type="number"
                  min={1}
                  value={viewport.width}
                  onChange={(e) =>
                    setConfig({
                      ...config,
                      display: {
                        ...config.display,
                        viewport: { ...viewport, width: Number(e.target.value) },
                      },
                    })
                  }
                  className="mt-1 block w-full rounded border border-zinc-700 bg-zinc-800 px-3 py-2 text-sm"
                />
              </label>
              <label className="block">
                <span className="text-sm text-zinc-400">Height</span>
                <input
                  type="number"
                  min={1}
                  value={viewport.height}
                  onChange={(e) =>
                    setConfig({
                      ...config,
                      display: {
                        ...config.display,
                        viewport: { ...viewport, height: Number(e.target.value) },
                      },
                    })
                  }
                  className="mt-1 block w-full rounded border border-zinc-700 bg-zinc-800 px-3 py-2 text-sm"
                />
              </label>
            </div>
          )}
          <label className="flex items-center gap-2">
            <input
              type="checkbox"
              checked={config.display.orientation_landscape}
              onChange={(e) =>
                setConfig({
                  ...config,
                  display: { ...config.display, orientation_landscape: e.target.checked },
                })
              }
              className="rounded border-zinc-700"
            />
            <span className="text-sm">Landscape orientation</span>
          </label>
        </fieldset>

        <fieldset className="space-y-3">
          <legend className="text-lg font-semibold">Audio Mix</legend>
          <div className="grid grid-cols-2 gap-3">
            {config.audio.channel_mix.map((ch, i) => (
              <div key={i} className="rounded border border-zinc-700 p-2">
                <p className="text-xs text-zinc-400 mb-1">Channel {i}</p>
                <div className="flex gap-2">
                  <label className="block flex-1">
                    <span className="text-xs text-zinc-500">L%</span>
                    <input
                      type="number"
                      min={0}
                      max={100}
                      value={ch.left_pct}
                      onChange={(e) => {
                        const mix = [...config.audio.channel_mix] as MachineConfig['audio']['channel_mix'];
                        mix[i] = { ...mix[i], left_pct: Number(e.target.value) };
                        setConfig({ ...config, audio: { ...config.audio, channel_mix: mix } });
                      }}
                      className="block w-full rounded border border-zinc-700 bg-zinc-800 px-2 py-1 text-sm"
                    />
                  </label>
                  <label className="block flex-1">
                    <span className="text-xs text-zinc-500">R%</span>
                    <input
                      type="number"
                      min={0}
                      max={100}
                      value={ch.right_pct}
                      onChange={(e) => {
                        const mix = [...config.audio.channel_mix] as MachineConfig['audio']['channel_mix'];
                        mix[i] = { ...mix[i], right_pct: Number(e.target.value) };
                        setConfig({ ...config, audio: { ...config.audio, channel_mix: mix } });
                      }}
                      className="block w-full rounded border border-zinc-700 bg-zinc-800 px-2 py-1 text-sm"
                    />
                  </label>
                </div>
              </div>
            ))}
          </div>
        </fieldset>

        <button
          type="submit"
          disabled={saving}
          className="rounded bg-amber-600 px-4 py-2 text-sm font-medium hover:bg-amber-500 disabled:opacity-50"
        >
          {saving ? 'Saving…' : 'Save Configuration'}
        </button>
      </form>
    </div>
  );
}

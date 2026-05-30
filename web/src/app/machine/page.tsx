'use client';

import { useEffect, useState } from 'react';
import {
  getMachineConfig,
  updateMachineConfig,
  startMachine,
  stopMachine,
  getMachineStatus,
  type MachineConfig,
  type MachineStatus,
  type AmigaModel,
  type ScalingMode,
  type ViewportMode,
  type FloppySpeedPercent,
} from '@/lib/api';

const DEFAULT_VIEWPORT: MachineConfig['display']['viewport'] = {
  mode: 'Auto',
  x: 0,
  y: 0,
  width: 754,
  height: 288,
  vertical_stretch: true,
};
const DEFAULT_FLOPPY_SPEED_PERCENT: FloppySpeedPercent = 100;
const FLOPPY_SPEED_OPTIONS: FloppySpeedPercent[] = [100, 200, 400, 800, 0];

function isFloppySpeedPercent(value: unknown): value is FloppySpeedPercent {
  return (
    typeof value === 'number' &&
    (value === 0 || value === 100 || value === 200 || value === 400 || value === 800)
  );
}

function normalizeConfig(config: MachineConfig): MachineConfig {
  return {
    ...config,
    floppy_speed_percent: isFloppySpeedPercent(config.floppy_speed_percent)
      ? config.floppy_speed_percent
      : DEFAULT_FLOPPY_SPEED_PERCENT,
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

  if (!config) {
    return <p className="text-zinc-400">{error ?? 'Loading…'}</p>;
  }

  const viewport = config.display.viewport ?? DEFAULT_VIEWPORT;

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between">
        <h1 className="text-2xl font-bold">Machine</h1>
        <div className="flex gap-2">
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
            <span className="text-sm text-zinc-400">Viewport</span>
            <select
              value={viewport.mode}
              onChange={(e) =>
                setConfig({
                  ...config,
                  display: {
                    ...config.display,
                    viewport: { ...viewport, mode: e.target.value as ViewportMode },
                  },
                })
              }
              className="mt-1 block w-full rounded border border-zinc-700 bg-zinc-800 px-3 py-2 text-sm"
            >
              <option value="Auto">Auto</option>
              <option value="Raw">Raw</option>
              <option value="Manual">Manual</option>
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
            <span className="text-sm">Stretch viewport vertically</span>
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

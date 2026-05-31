/* eslint-disable @next/next/no-img-element -- live emulator screenshots are served by the local REST endpoint. */
'use client';

import { useEffect, useState, useRef } from 'react';
import {
  getMachineStatus,
  getMachineConfig,
  type MachineStatus,
  type MachineConfig,
  startMachine,
  stopMachine,
} from '@/lib/api';

function errorMessage(error: unknown, fallback: string): string {
  return error instanceof Error ? error.message : fallback;
}

function networkBackendLabel(backend: MachineConfig['network']['backend']): string {
  return backend === 'Slirp' ? 'SLIRP / NAT' : 'Disabled';
}

function networkSummary(status: MachineStatus | null, config: MachineConfig): string {
  if (!status) return networkBackendLabel(config.network.backend);
  return `${networkBackendLabel(status.network.backend)} (${status.network.link_up ? 'link up' : 'link down'})`;
}

export default function DashboardPage() {
  const [status, setStatus] = useState<MachineStatus | null>(null);
  const [config, setConfig] = useState<MachineConfig | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [audioSeparation, setAudioSeparation] = useState<number>(100);
  const [screenshotUrl, setScreenshotUrl] = useState<string>('/api/machine/screenshot?t=0');
  const [autoRefresh, setAutoRefresh] = useState<boolean>(true);
  const refreshInterval = useRef<NodeJS.Timeout | null>(null);

  // Poll status & config on load
  const loadData = () => {
    getMachineStatus()
      .then((r) => {
        if (r.success && r.data) setStatus(r.data);
      })
      .catch((e: Error) => setError(e.message));

    getMachineConfig()
      .then((r) => {
        if (r.success && r.data) {
          setConfig(r.data);
          setAudioSeparation(r.data.audio.stereo_separation);
        }
      })
      .catch((e: Error) => setError(e.message));
  };

  useEffect(() => {
    loadData();
    const interval = setInterval(loadData, 1000);
    return () => clearInterval(interval);
  }, []);

  // Handle screenshot auto-refresh
  useEffect(() => {
    if (autoRefresh) {
      refreshInterval.current = setInterval(() => {
        setScreenshotUrl(`/api/machine/screenshot?t=${Date.now()}`);
      }, 200);
    } else {
      if (refreshInterval.current) {
        clearInterval(refreshInterval.current);
      }
    }
    return () => {
      if (refreshInterval.current) clearInterval(refreshInterval.current);
    };
  }, [autoRefresh]);

  const handlePlayPause = async () => {
    if (!status) return;
    try {
      if (status.running) {
        await stopMachine(); // POST /api/machine/stop maps to pausing
      } else {
        await startMachine(); // POST /api/machine/start maps to resuming
      }
      loadData();
    } catch (e: unknown) {
      setError(errorMessage(e, 'Machine control failed'));
    }
  };

  const handleReset = async () => {
    try {
      await fetch('/api/machine/reset', { method: 'POST' });
      loadData();
    } catch (e: unknown) {
      setError(errorMessage(e, 'Reset failed'));
    }
  };

  const handleSeparationChange = async (val: number) => {
    setAudioSeparation(val);
    try {
      await fetch('/api/machine/audio/separation', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ separation: val }),
      });
    } catch (e: unknown) {
      setError(errorMessage(e, 'Audio update failed'));
    }
  };

  return (
    <div className="space-y-8 max-w-7xl mx-auto p-4 sm:p-6 lg:p-8 text-zinc-100">
      {/* Premium Glassmorphic Header */}
      <div className="relative rounded-2xl p-6 md:p-8 overflow-hidden backdrop-blur-md bg-zinc-900/60 border border-zinc-800/80 shadow-2xl flex flex-col md:flex-row md:items-center justify-between gap-6 transition-all duration-300">
        <div className="absolute top-0 right-0 w-64 h-64 bg-amber-500/10 rounded-full filter blur-3xl pointer-events-none" />
        <div className="absolute bottom-0 left-0 w-64 h-64 bg-indigo-500/10 rounded-full filter blur-3xl pointer-events-none" />

        <div className="space-y-2">
          <div className="flex items-center gap-3">
            <span className="h-2.5 w-2.5 rounded-full bg-gradient-to-r from-amber-500 to-rose-500 animate-pulse shadow-[0_0_10px_rgba(245,158,11,0.6)]" />
            <h1 className="text-3xl font-extrabold tracking-tight bg-gradient-to-r from-zinc-100 via-zinc-200 to-zinc-400 bg-clip-text text-transparent">
              Rumiga Control Center
            </h1>
          </div>
          <p className="text-sm text-zinc-400 font-medium">
            AGA Emulation &amp; Diagnostics Engine
          </p>
        </div>

        {status && (
          <div className="flex items-center gap-4 bg-zinc-950/40 px-5 py-3 rounded-xl border border-zinc-800/40">
            <div className="space-y-0.5">
              <p className="text-xs text-zinc-400 uppercase tracking-widest font-semibold">Model Profile</p>
              <p className="text-sm font-bold text-zinc-200">{status.model}</p>
            </div>
            <div className="h-8 w-px bg-zinc-800" />
            <div className="space-y-0.5">
              <p className="text-xs text-zinc-400 uppercase tracking-widest font-semibold">Status</p>
              <p className={`text-sm font-bold ${status.running ? 'text-emerald-400' : 'text-zinc-500'}`}>
                {status.running ? `Active (${status.fps.toFixed(1)} FPS)` : 'Paused'}
              </p>
            </div>
          </div>
        )}
      </div>

      {error && (
        <div className="rounded-xl bg-red-950/30 border border-red-900/60 p-4 text-sm text-red-400 flex items-center gap-3">
          <svg className="h-5 w-5 shrink-0" fill="none" viewBox="0 0 24 24" stroke="currentColor">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-3L13.732 4c-.77-1.333-2.694-1.333-3.464 0L3.34 16c-.77 1.333.192 3 1.732 3z" />
          </svg>
          {error}
        </div>
      )}

      {/* Grid Layout */}
      <div className="grid gap-8 lg:grid-cols-12">
        {/* Left Column: Screenshot Preview (7 cols) */}
        <div className="lg:col-span-7 space-y-4 flex flex-col">
          <div className="flex items-center justify-between">
            <h2 className="text-xl font-bold tracking-tight text-zinc-200">Live Guest Display</h2>
            <button
              onClick={() => setAutoRefresh(!autoRefresh)}
              className={`text-xs px-3 py-1.5 rounded-lg border font-medium transition-all ${autoRefresh ? 'bg-amber-500/10 border-amber-500/30 text-amber-400' : 'bg-zinc-900/40 border-zinc-800 text-zinc-400 hover:border-zinc-700'}`}
            >
              {autoRefresh ? '● Auto Refreshing' : 'Paused Refresh'}
            </button>
          </div>

          <div className="relative rounded-2xl overflow-hidden border border-zinc-800/80 bg-zinc-950 shadow-2xl flex-1 flex items-center justify-center min-h-[360px] group">
            <img
              src={screenshotUrl}
              alt="Amiga Screen Live Preview"
              className="w-full h-auto object-contain max-h-[480px] transition-transform duration-300 group-hover:scale-[1.005]"
              onError={(e) => {
                // fallback to a clean display placeholder if not running
                e.currentTarget.src = "data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='100' height='100' viewBox='0 0 100 100'%3E%3Crect width='100' height='100' fill='%2309090b'/%3E%3Ctext x='50' y='50' fill='%2327272a' font-family='sans-serif' font-size='4' text-anchor='middle' dominant-baseline='middle'%3ENo Display Signal%3C/text%3E%3C/svg%3E";
              }}
            />
          </div>
        </div>

        {/* Right Column: Controls & System Config (5 cols) */}
        <div className="lg:col-span-5 space-y-6">
          <h2 className="text-xl font-bold tracking-tight text-zinc-200">System Parameters</h2>

          {/* Glassmorphic Control Center Card */}
          <div className="rounded-2xl p-6 backdrop-blur-md bg-zinc-900/60 border border-zinc-800/80 shadow-xl space-y-6">
            <div className="space-y-4">
              <h3 className="text-sm font-semibold uppercase tracking-wider text-zinc-400">Emulation Controls</h3>
              <div className="grid grid-cols-2 gap-4">
                <button
                  onClick={handlePlayPause}
                  className={`flex items-center justify-center gap-2 py-3.5 px-4 rounded-xl font-bold transition-all duration-200 hover:scale-[1.02] active:scale-[0.98] cursor-pointer ${status?.running ? 'bg-zinc-850 hover:bg-zinc-800 text-zinc-100 border border-zinc-700/50' : 'bg-gradient-to-r from-amber-500 to-rose-500 hover:from-amber-600 hover:to-rose-600 text-zinc-950 font-extrabold shadow-[0_0_15px_rgba(245,158,11,0.3)]'}`}
                >
                  {status?.running ? (
                    <>
                      <span className="h-2 w-2 rounded-full bg-zinc-400" />
                      Pause State
                    </>
                  ) : (
                    <>
                      <span className="h-2 w-2 rounded-full bg-zinc-950 animate-ping" />
                      Run Machine
                    </>
                  )}
                </button>
                <button
                  onClick={handleReset}
                  className="flex items-center justify-center gap-2 py-3.5 px-4 rounded-xl bg-zinc-900 border border-zinc-800 hover:border-zinc-700 hover:bg-zinc-800 text-zinc-200 font-bold transition-all duration-200 hover:scale-[1.02] active:scale-[0.98] cursor-pointer"
                >
                  <svg className="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2.2} d="M4 4v5h.582m15.356 2A8.001 8.001 0 1121.21 7.89M9 11l3-3 3 3m-3-3v12" />
                  </svg>
                  System Reset
                </button>
              </div>
            </div>

            <div className="h-px bg-zinc-800" />

            {/* Apple UX Audio Separation Card */}
            <div className="space-y-4">
              <div className="flex items-center justify-between">
                <h3 className="text-sm font-semibold uppercase tracking-wider text-zinc-400">Headphone Stereo separation</h3>
                <span className="text-xs px-2.5 py-1 rounded-full bg-amber-500/10 text-amber-400 font-bold border border-amber-500/20">
                  {audioSeparation === 100 ? 'Authentic (100%)' : audioSeparation === 0 ? 'Mono (0%)' : `Blend (${audioSeparation}%)`}
                </span>
              </div>
              <p className="text-xs text-zinc-400 leading-relaxed">
                Mitigate headphone listener fatigue. Scale dynamically from standard 100% hard-pan stereo down to 0% mono.
              </p>
              <div className="space-y-2">
                <input
                  type="range"
                  min="0"
                  max="100"
                  value={audioSeparation}
                  onChange={(e) => handleSeparationChange(Number(e.target.value))}
                  className="w-full h-2 bg-zinc-950 rounded-lg appearance-none cursor-pointer accent-amber-500"
                />
                <div className="flex justify-between text-[10px] text-zinc-500 font-bold uppercase tracking-wider">
                  <span>Mono</span>
                  <span>Soft Panning</span>
                  <span>Authentic</span>
                </div>
              </div>
            </div>
          </div>

          {/* System Info Block */}
          {config && (
            <div className="rounded-2xl p-6 backdrop-blur-md bg-zinc-900/60 border border-zinc-800/80 shadow-xl space-y-4">
              <h3 className="text-sm font-semibold uppercase tracking-wider text-zinc-400">Memory &amp; Silicon Config</h3>

              <div className="space-y-3">
                <div className="flex items-center justify-between text-sm py-1.5 border-b border-zinc-800/40">
                  <span className="text-zinc-400 font-medium">ROM Image</span>
                  <span className="font-mono text-zinc-200 text-xs truncate max-w-[200px]" title={config.rom_file}>
                    {config.rom_file.split('/').pop() || 'None Loaded'}
                  </span>
                </div>
                <div className="flex items-center justify-between text-sm py-1.5 border-b border-zinc-800/40">
                  <span className="text-zinc-400 font-medium">Gayle HDF</span>
                  <span className="font-mono text-zinc-200 text-xs truncate max-w-[200px]" title={config.hdf_path ?? ''}>
                    {config.hdf_path?.split('/').pop() || 'None Mounted'}
                  </span>
                </div>
                <div className="flex items-center justify-between text-sm py-1.5 border-b border-zinc-800/40">
                  <span className="text-zinc-400 font-medium">HDF Policy</span>
                  <span className="font-bold text-zinc-200">
                    {config.hdf_write_policy === 'Writeback' ? 'Writeback' : 'Read-only session'}
                  </span>
                </div>
                <div className="flex items-center justify-between text-sm py-1.5 border-b border-zinc-800/40">
                  <span className="text-zinc-400 font-medium">A2065 Network</span>
                  <span className="font-bold text-zinc-200">
                    {networkSummary(status, config)}
                  </span>
                </div>
                <div className="flex items-center justify-between text-sm py-1.5 border-b border-zinc-800/40">
                  <span className="text-zinc-400 font-medium">Chip RAM (Graphics)</span>
                  <span className="font-bold text-zinc-200">{config.chip_ram_kb / 1024} MB</span>
                </div>
                <div className="flex items-center justify-between text-sm py-1.5 border-b border-zinc-800/40">
                  <span className="text-zinc-400 font-medium">Slow RAM (Trapdoor)</span>
                  <span className="font-bold text-zinc-200">{config.slow_ram_kb} KB</span>
                </div>
                <div className="flex items-center justify-between text-sm py-1.5 border-b border-zinc-800/40">
                  <span className="text-zinc-400 font-medium">Fast RAM (Zorro II)</span>
                  <span className="font-bold text-zinc-200">{config.fast_ram_kb / 1024} MB</span>
                </div>
                <div className="flex items-center justify-between text-sm py-1.5">
                  <span className="text-zinc-400 font-medium">Floppy Speed</span>
                  <span className="font-bold text-amber-400">
                    {config.floppy_speed_percent === 0 ? 'Turbo Mode' : `${config.floppy_speed_percent}% compatible`}
                  </span>
                </div>
              </div>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

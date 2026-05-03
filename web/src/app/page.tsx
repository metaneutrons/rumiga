'use client';

import { useEffect, useState } from 'react';
import { getMachineStatus, type MachineStatus } from '@/lib/api';

export default function DashboardPage() {
  const [status, setStatus] = useState<MachineStatus | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    getMachineStatus()
      .then((r) => {
        if (r.success && r.data) setStatus(r.data);
        else setError(r.error ?? 'Failed to load status');
      })
      .catch((e: Error) => setError(e.message));
  }, []);

  return (
    <div className="space-y-6">
      <h1 className="text-2xl font-bold">Dashboard</h1>

      {error && <p className="text-red-400">{error}</p>}

      {status && (
        <div className="rounded-lg border border-zinc-700 p-4 space-y-2">
          <p>
            <span className="text-zinc-400">Model:</span> {status.model}
          </p>
          <p>
            <span className="text-zinc-400">Status:</span>{' '}
            {status.running ? (
              <span className="text-green-400">Running ({status.fps.toFixed(1)} fps)</span>
            ) : (
              <span className="text-zinc-500">Stopped</span>
            )}
          </p>
        </div>
      )}

      <div className="grid gap-4 sm:grid-cols-3">
        <a
          href="/files/"
          className="rounded-lg border border-zinc-700 p-4 hover:border-amber-500 transition-colors"
        >
          <h2 className="font-semibold">Files</h2>
          <p className="text-sm text-zinc-400">Manage ROMs and disk images</p>
        </a>
        <a
          href="/wifi/"
          className="rounded-lg border border-zinc-700 p-4 hover:border-amber-500 transition-colors"
        >
          <h2 className="font-semibold">WiFi</h2>
          <p className="text-sm text-zinc-400">Network configuration</p>
        </a>
        <a
          href="/machine/"
          className="rounded-lg border border-zinc-700 p-4 hover:border-amber-500 transition-colors"
        >
          <h2 className="font-semibold">Machine</h2>
          <p className="text-sm text-zinc-400">Emulator settings</p>
        </a>
      </div>
    </div>
  );
}

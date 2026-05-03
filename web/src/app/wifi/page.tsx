'use client';

import { useEffect, useState } from 'react';
import {
  getWifiStatus,
  scanWifi,
  connectWifi,
  type WifiStatus,
  type WifiNetwork,
} from '@/lib/api';

export default function WifiPage() {
  const [status, setStatus] = useState<WifiStatus | null>(null);
  const [networks, setNetworks] = useState<WifiNetwork[]>([]);
  const [scanning, setScanning] = useState(false);
  const [ssid, setSsid] = useState('');
  const [password, setPassword] = useState('');
  const [error, setError] = useState<string | null>(null);
  const [connecting, setConnecting] = useState(false);

  useEffect(() => {
    getWifiStatus()
      .then((r) => {
        if (r.success && r.data) setStatus(r.data);
        else setError(r.error ?? 'Failed to load WiFi status');
      })
      .catch((e: Error) => setError(e.message));
  }, []);

  async function handleScan() {
    setScanning(true);
    setError(null);
    try {
      const r = await scanWifi();
      if (r.success && r.data) setNetworks(r.data.networks);
      else setError(r.error ?? 'Scan failed');
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : 'Scan failed');
    } finally {
      setScanning(false);
    }
  }

  async function handleConnect(e: React.FormEvent) {
    e.preventDefault();
    setConnecting(true);
    setError(null);
    try {
      const r = await connectWifi(ssid, password);
      if (!r.success) setError(r.error ?? 'Connection failed');
      else {
        const s = await getWifiStatus();
        if (s.success && s.data) setStatus(s.data);
      }
    } catch (err: unknown) {
      setError(err instanceof Error ? err.message : 'Connection failed');
    } finally {
      setConnecting(false);
    }
  }

  return (
    <div className="space-y-6">
      <h1 className="text-2xl font-bold">WiFi</h1>

      {error && <p className="text-red-400">{error}</p>}

      {status && (
        <div className="rounded-lg border border-zinc-700 p-4 space-y-1">
          <p>
            <span className="text-zinc-400">Mode:</span> {status.mode}
          </p>
          <p>
            <span className="text-zinc-400">Connected:</span>{' '}
            {status.connected ? (
              <span className="text-green-400">{status.ssid}</span>
            ) : (
              <span className="text-zinc-500">No</span>
            )}
          </p>
          {status.ip && (
            <p>
              <span className="text-zinc-400">IP:</span> {status.ip}
            </p>
          )}
        </div>
      )}

      <div className="space-y-3">
        <button
          onClick={handleScan}
          disabled={scanning}
          className="rounded bg-amber-600 px-3 py-1.5 text-sm font-medium hover:bg-amber-500 disabled:opacity-50"
        >
          {scanning ? 'Scanning…' : 'Scan Networks'}
        </button>

        {networks.length > 0 && (
          <ul className="divide-y divide-zinc-800 rounded-lg border border-zinc-700">
            {networks.map((n) => (
              <li
                key={n.ssid}
                className="flex items-center justify-between px-4 py-2 cursor-pointer hover:bg-zinc-800"
                onClick={() => setSsid(n.ssid)}
              >
                <span>{n.ssid}</span>
                <span className="text-xs text-zinc-500">
                  {n.rssi} dBm {n.secured && '🔒'}
                </span>
              </li>
            ))}
          </ul>
        )}
      </div>

      <form onSubmit={handleConnect} className="space-y-3 max-w-sm">
        <h2 className="text-lg font-semibold">Connect</h2>
        <input
          type="text"
          placeholder="SSID"
          value={ssid}
          onChange={(e) => setSsid(e.target.value)}
          required
          className="w-full rounded border border-zinc-700 bg-zinc-800 px-3 py-2 text-sm"
        />
        <input
          type="password"
          placeholder="Password"
          value={password}
          onChange={(e) => setPassword(e.target.value)}
          className="w-full rounded border border-zinc-700 bg-zinc-800 px-3 py-2 text-sm"
        />
        <button
          type="submit"
          disabled={connecting || !ssid}
          className="rounded bg-amber-600 px-4 py-2 text-sm font-medium hover:bg-amber-500 disabled:opacity-50"
        >
          {connecting ? 'Connecting…' : 'Connect'}
        </button>
      </form>
    </div>
  );
}

'use client';

import { useCallback, useEffect, useRef, useState } from 'react';
import { deleteFile, formatSd, getFiles, uploadFile, type FileEntry } from '@/lib/api';

export default function FilesPage() {
  const [files, setFiles] = useState<FileEntry[]>([]);
  const [freeBytes, setFreeBytes] = useState(0);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const inputRef = useRef<HTMLInputElement>(null);

  const load = useCallback(async (showLoading = true) => {
    if (showLoading) {
      setLoading(true);
    }
    try {
      const r = await getFiles('/');
      if (r.success && r.data) {
        setFiles(r.data.files);
        setFreeBytes(r.data.free_bytes);
      } else {
        setError(r.error ?? 'Failed to load files');
      }
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : 'Failed to load files');
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    const timer = window.setTimeout(() => {
      void load(false);
    }, 0);
    return () => window.clearTimeout(timer);
  }, [load]);

  async function handleUpload(e: React.ChangeEvent<HTMLInputElement>) {
    const file = e.target.files?.[0];
    if (!file) return;
    try {
      const r = await uploadFile(file);
      if (!r.success) setError(r.error ?? 'Upload failed');
      else void load();
    } catch (err: unknown) {
      setError(err instanceof Error ? err.message : 'Upload failed');
    }
  }

  async function handleDelete(name: string) {
    if (!confirm(`Delete ${name}?`)) return;
    try {
      const r = await deleteFile(name);
      if (!r.success) setError(r.error ?? 'Delete failed');
      else void load();
    } catch (err: unknown) {
      setError(err instanceof Error ? err.message : 'Delete failed');
    }
  }

  async function handleFormat() {
    const token = prompt('Type CONFIRM to format the SD card:');
    if (token !== 'CONFIRM') return;
    try {
      const r = await formatSd(token);
      if (!r.success) setError(r.error ?? 'Format failed');
      else void load();
    } catch (err: unknown) {
      setError(err instanceof Error ? err.message : 'Format failed');
    }
  }

  function formatSize(bytes: number): string {
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  }

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between">
        <h1 className="text-2xl font-bold">Files</h1>
        <span className="text-sm text-zinc-400">{formatSize(freeBytes)} free</span>
      </div>

      {error && <p className="text-red-400">{error}</p>}

      <div className="flex gap-2">
        <input ref={inputRef} type="file" className="hidden" onChange={handleUpload} />
        <button
          onClick={() => inputRef.current?.click()}
          className="rounded bg-amber-600 px-3 py-1.5 text-sm font-medium hover:bg-amber-500"
        >
          Upload
        </button>
        <button
          onClick={handleFormat}
          className="rounded bg-red-700 px-3 py-1.5 text-sm font-medium hover:bg-red-600"
        >
          Format SD
        </button>
      </div>

      {loading ? (
        <p className="text-zinc-400">Loading…</p>
      ) : (
        <ul className="divide-y divide-zinc-800 rounded-lg border border-zinc-700">
          {files.map((f) => (
            <li key={f.name} className="flex items-center justify-between px-4 py-2">
              <div>
                <span className={f.is_directory ? 'text-amber-300' : ''}>{f.name}</span>
                {!f.is_directory && (
                  <span className="ml-2 text-xs text-zinc-500">{formatSize(f.size)}</span>
                )}
              </div>
              {!f.is_directory && (
                <button
                  onClick={() => handleDelete(f.name)}
                  className="text-xs text-red-400 hover:text-red-300"
                >
                  Delete
                </button>
              )}
            </li>
          ))}
          {files.length === 0 && (
            <li className="px-4 py-2 text-zinc-500">No files</li>
          )}
        </ul>
      )}
    </div>
  );
}

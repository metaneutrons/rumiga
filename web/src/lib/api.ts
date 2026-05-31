// API types mirroring crates/rumiga-api/src/lib.rs

export interface FileEntry {
  name: string;
  size: number;
  is_directory: boolean;
}

export interface FileListResponse {
  path: string;
  files: FileEntry[];
  total_bytes: number;
  free_bytes: number;
}

export interface WifiNetwork {
  ssid: string;
  rssi: number;
  secured: boolean;
}

export interface WifiStatus {
  connected: boolean;
  ssid: string | null;
  ip: string | null;
  mode: 'SoftAp' | 'Client' | 'Disconnected';
}

export interface WifiScanResponse {
  networks: WifiNetwork[];
}

export type AmigaModel = 'A500' | 'A500Plus' | 'A1200';

export interface ChannelMixConfig {
  left_pct: number;
  right_pct: number;
}

export interface AudioConfig {
  channel_mix: [ChannelMixConfig, ChannelMixConfig, ChannelMixConfig, ChannelMixConfig];
  stereo_separation: number;
}

export type ScalingMode = 'Integer' | 'AspectFit' | 'Stretch';
export type ViewportMode = 'Raw' | 'Auto' | 'Manual';
export type ViewportPreset = 'NativeFullBorder' | 'VisibleArea' | 'Overscan' | 'AutoCenter';
export type FloppySpeedPercent = 0 | 100 | 200 | 400 | 800;
export type HdfWritePolicy = 'ReadOnly' | 'Writeback';

export interface ViewportConfig {
  mode: ViewportMode;
  preset: ViewportPreset;
  x: number;
  y: number;
  width: number;
  height: number;
  vertical_stretch: boolean;
}

export interface DisplayConfig {
  scaling: ScalingMode;
  orientation_landscape: boolean;
  viewport: ViewportConfig;
}

export interface MachineConfig {
  model: AmigaModel;
  chip_ram_kb: number;
  slow_ram_kb: number;
  fast_ram_kb: number;
  rom_file: string;
  floppy: [string | null, string | null, string | null, string | null];
  floppy_speed_percent: FloppySpeedPercent;
  hdf_path: string | null;
  hdf_write_policy: HdfWritePolicy;
  audio: AudioConfig;
  display: DisplayConfig;
}

export interface MachineStatus {
  running: boolean;
  fps: number;
  model: AmigaModel;
}

export interface ApiResponse<T> {
  success: boolean;
  data: T | null;
  error: string | null;
}

// ─── Client ──────────────────────────────────────────────────────────────────

const BASE_URL = typeof window !== 'undefined' ? window.location.origin : '';

async function request<T>(path: string, init?: RequestInit): Promise<T> {
  const res = await fetch(`${BASE_URL}${path}`, init);
  if (!res.ok) {
    const body = await res.text();
    throw new Error(body || `HTTP ${res.status}`);
  }
  return res.json() as Promise<T>;
}

function json(body: unknown): RequestInit {
  return {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  };
}

// ─── Files ───────────────────────────────────────────────────────────────────

export function getFiles(path: string): Promise<ApiResponse<FileListResponse>> {
  return request(`/api/files?path=${encodeURIComponent(path)}`);
}

export function uploadFile(file: File): Promise<ApiResponse<null>> {
  const form = new FormData();
  form.append('file', file);
  return request('/api/files/upload', { method: 'POST', body: form });
}

export function deleteFile(name: string): Promise<ApiResponse<null>> {
  return request(`/api/files/${encodeURIComponent(name)}`, { method: 'DELETE' });
}

export function formatSd(token: string): Promise<ApiResponse<null>> {
  return request('/api/files/format', json({ confirm_token: token }));
}

// ─── WiFi ────────────────────────────────────────────────────────────────────

export function getWifiStatus(): Promise<ApiResponse<WifiStatus>> {
  return request('/api/wifi/status');
}

export function scanWifi(): Promise<ApiResponse<WifiScanResponse>> {
  return request('/api/wifi/scan', { method: 'POST' });
}

export function connectWifi(ssid: string, password: string): Promise<ApiResponse<null>> {
  return request('/api/wifi/connect', json({ ssid, password }));
}

// ─── Machine ─────────────────────────────────────────────────────────────────

export function getMachineConfig(): Promise<ApiResponse<MachineConfig>> {
  return request('/api/machine/config');
}

export function updateMachineConfig(config: MachineConfig): Promise<ApiResponse<null>> {
  return request('/api/machine/config', { method: 'PUT', ...json(config) });
}

export function startMachine(): Promise<ApiResponse<null>> {
  return request('/api/machine/start', { method: 'POST' });
}

export function stopMachine(): Promise<ApiResponse<null>> {
  return request('/api/machine/stop', { method: 'POST' });
}

export function getMachineStatus(): Promise<ApiResponse<MachineStatus>> {
  return request('/api/machine/status');
}

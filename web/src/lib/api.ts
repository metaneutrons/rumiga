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

export interface FormatRequest {
  confirm_token: string;
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
  mode: WifiMode;
}

export interface WifiScanResponse {
  networks: WifiNetwork[];
}

export interface WifiConnectRequest {
  ssid: string;
  password: string;
}

export type WifiMode = 'SoftAp' | 'Client' | 'Disconnected';
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
export type NetworkDevice = 'A2065';
export type NetworkBackend = 'Disabled' | 'Slirp';

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

export interface NetworkConfig {
  device: NetworkDevice;
  backend: NetworkBackend;
  mac_address: string;
}

export interface NetworkPacketCounters {
  tx_packets: number;
  rx_packets: number;
  dropped_packets: number;
}

export interface NetworkStatus {
  enabled: boolean;
  device: NetworkDevice;
  backend: NetworkBackend;
  mac_address: string;
  a2065_present: boolean;
  a2065_configured: boolean;
  a2065_shut_up: boolean;
  a2065_base_address: string | null;
  a2065_card_mac_address: string;
  link_up: boolean;
  counters: NetworkPacketCounters;
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
  network: NetworkConfig;
}

export interface MachineStatus {
  running: boolean;
  fps: number;
  model: AmigaModel;
  network: NetworkStatus;
}

export interface SupportMachineSummary {
  model: AmigaModel;
  chip_ram_kb: number;
  slow_ram_kb: number;
  fast_ram_kb: number;
  floppy_speed_percent: FloppySpeedPercent;
  hdf_write_policy: HdfWritePolicy;
}

export interface SupportMediaSummary {
  rom_name: string | null;
  hdf_name: string | null;
  floppies: [string | null, string | null, string | null, string | null];
}

export interface SupportScreenshotSummary {
  available: boolean;
  width: number;
  height: number;
  endpoint: string;
  pixel_format: string;
}

export interface SupportBundle {
  schema: string;
  machine: SupportMachineSummary;
  status: MachineStatus;
  display: DisplayConfig;
  media: SupportMediaSummary;
  screenshot: SupportScreenshotSummary;
  notes: string[];
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

export function getSupportBundle(): Promise<ApiResponse<SupportBundle>> {
  return request('/api/machine/support-bundle');
}

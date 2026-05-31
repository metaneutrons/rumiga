// API types mirroring crates/rumiga-api/src/lib.rs

export const API_RESPONSE_SCHEMA_ID = 'rumiga.api.response.v1';
export const API_RESPONSE_SCHEMA_VERSION = 1;

export const API_PATHS = {
  machineStatus: '/api/machine/status',
  machineConfig: '/api/machine/config',
  machineSupportBundle: '/api/machine/support-bundle',
  machineReset: '/api/machine/reset',
  machinePause: '/api/machine/pause',
  machineResume: '/api/machine/resume',
  machineStart: '/api/machine/start',
  machineStop: '/api/machine/stop',
  machineFloppyInsert: '/api/machine/floppy/insert',
  machineFloppyEject: '/api/machine/floppy/eject',
  machineAudioSeparation: '/api/machine/audio/separation',
  machineScreenshot: '/api/machine/screenshot',
  files: '/api/files',
  filesUpload: '/api/files/upload',
  filesDelete: '/api/files/:name',
  filesFormat: '/api/files/format',
  wifiStatus: '/api/wifi/status',
  wifiScan: '/api/wifi/scan',
  wifiConnect: '/api/wifi/connect',
} as const;

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

export interface FloppyInsertRequest {
  drive_idx: number;
  path: string;
}

export interface FloppyEjectRequest {
  drive_idx: number;
}

export interface AudioSeparationRequest {
  separation: number;
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
export type ScreenshotKind = 'NativeFramebuffer' | 'ViewportPresentation';
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
  kind: ScreenshotKind;
  width: number;
  height: number;
  endpoint: string;
  pixel_format: string;
  available_kinds: ScreenshotKind[];
  native_width: number;
  native_height: number;
  presentation_width: number;
  presentation_height: number;
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

export type ApiResponseFormat = 'Json' | 'Png';

export interface ApiEndpoint {
  method: string;
  path: string;
  response_format: ApiResponseFormat;
}

export const API_ENDPOINTS = [
  { method: 'GET', path: '/api/machine/status', response_format: 'Json' },
  { method: 'GET', path: '/api/machine/config', response_format: 'Json' },
  { method: 'PUT', path: '/api/machine/config', response_format: 'Json' },
  { method: 'GET', path: '/api/machine/support-bundle', response_format: 'Json' },
  { method: 'POST', path: '/api/machine/reset', response_format: 'Json' },
  { method: 'POST', path: '/api/machine/pause', response_format: 'Json' },
  { method: 'POST', path: '/api/machine/resume', response_format: 'Json' },
  { method: 'POST', path: '/api/machine/start', response_format: 'Json' },
  { method: 'POST', path: '/api/machine/stop', response_format: 'Json' },
  { method: 'POST', path: '/api/machine/floppy/insert', response_format: 'Json' },
  { method: 'POST', path: '/api/machine/floppy/eject', response_format: 'Json' },
  { method: 'POST', path: '/api/machine/audio/separation', response_format: 'Json' },
  { method: 'GET', path: '/api/machine/screenshot', response_format: 'Png' },
  { method: 'GET', path: '/api/files', response_format: 'Json' },
  { method: 'POST', path: '/api/files/upload', response_format: 'Json' },
  { method: 'DELETE', path: '/api/files/:name', response_format: 'Json' },
  { method: 'POST', path: '/api/files/format', response_format: 'Json' },
  { method: 'GET', path: '/api/wifi/status', response_format: 'Json' },
  { method: 'POST', path: '/api/wifi/scan', response_format: 'Json' },
  { method: 'POST', path: '/api/wifi/connect', response_format: 'Json' },
] as const satisfies readonly ApiEndpoint[];

export interface ApiResponse<T> {
  schema: string;
  version: number;
  success: boolean;
  data: T | null;
  error: string | null;
  error_code: string | null;
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

function json(body: unknown, method = 'POST'): RequestInit {
  return {
    method,
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  };
}

// ─── Files ───────────────────────────────────────────────────────────────────

export function getFiles(path: string): Promise<ApiResponse<FileListResponse>> {
  return request(`${API_PATHS.files}?path=${encodeURIComponent(path)}`);
}

export function uploadFile(file: File): Promise<ApiResponse<null>> {
  const form = new FormData();
  form.append('file', file);
  return request(API_PATHS.filesUpload, { method: 'POST', body: form });
}

export function deleteFile(name: string): Promise<ApiResponse<null>> {
  return request(API_PATHS.filesDelete.replace(':name', encodeURIComponent(name)), {
    method: 'DELETE',
  });
}

export function formatSd(token: string): Promise<ApiResponse<null>> {
  return request(API_PATHS.filesFormat, json({ confirm_token: token }));
}

// ─── WiFi ────────────────────────────────────────────────────────────────────

export function getWifiStatus(): Promise<ApiResponse<WifiStatus>> {
  return request(API_PATHS.wifiStatus);
}

export function scanWifi(): Promise<ApiResponse<WifiScanResponse>> {
  return request(API_PATHS.wifiScan, { method: 'POST' });
}

export function connectWifi(ssid: string, password: string): Promise<ApiResponse<null>> {
  return request(API_PATHS.wifiConnect, json({ ssid, password }));
}

// ─── Machine ─────────────────────────────────────────────────────────────────

export function getMachineConfig(): Promise<ApiResponse<MachineConfig>> {
  return request(API_PATHS.machineConfig);
}

export function updateMachineConfig(config: MachineConfig): Promise<ApiResponse<null>> {
  return request(API_PATHS.machineConfig, json(config, 'PUT'));
}

export function startMachine(): Promise<ApiResponse<null>> {
  return request(API_PATHS.machineStart, { method: 'POST' });
}

export function stopMachine(): Promise<ApiResponse<null>> {
  return request(API_PATHS.machineStop, { method: 'POST' });
}

export function resetMachine(): Promise<ApiResponse<null>> {
  return request(API_PATHS.machineReset, { method: 'POST' });
}

export function pauseMachine(): Promise<ApiResponse<null>> {
  return request(API_PATHS.machinePause, { method: 'POST' });
}

export function resumeMachine(): Promise<ApiResponse<null>> {
  return request(API_PATHS.machineResume, { method: 'POST' });
}

export function insertFloppy(requestBody: FloppyInsertRequest): Promise<ApiResponse<null>> {
  return request(API_PATHS.machineFloppyInsert, json(requestBody));
}

export function ejectFloppy(requestBody: FloppyEjectRequest): Promise<ApiResponse<null>> {
  return request(API_PATHS.machineFloppyEject, json(requestBody));
}

export function updateAudioSeparation(
  requestBody: AudioSeparationRequest,
): Promise<ApiResponse<null>> {
  return request(API_PATHS.machineAudioSeparation, json(requestBody));
}

export function machineScreenshotUrl(
  cacheBust: string | number = Date.now(),
  kind: ScreenshotKind = 'ViewportPresentation',
): string {
  const params = new URLSearchParams({
    t: String(cacheBust),
    kind,
  });
  return `${API_PATHS.machineScreenshot}?${params.toString()}`;
}

export function getMachineStatus(): Promise<ApiResponse<MachineStatus>> {
  return request(API_PATHS.machineStatus);
}

export function getSupportBundle(): Promise<ApiResponse<SupportBundle>> {
  return request(API_PATHS.machineSupportBundle);
}

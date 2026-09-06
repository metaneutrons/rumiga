import { afterEach, describe, expect, it, vi } from 'vitest';
import {
  API_ENDPOINTS,
  API_PATHS,
  connectWifi,
  deleteFile,
  ejectFloppy,
  formatSd,
  getFiles,
  getMachineConfig,
  getMachineStatus,
  getSupportBundle,
  getWifiStatus,
  insertFloppy,
  machineScreenshotUrl,
  pauseMachine,
  resetMachine,
  resumeMachine,
  scanWifi,
  startMachine,
  stopMachine,
  updateMachineConfig,
  updateAudioSeparation,
  uploadFile,
} from './api';

afterEach(() => vi.restoreAllMocks());

describe('API contract', () => {
  it('declares every endpoint with a supported response format', () => {
    expect(API_ENDPOINTS.length).toBeGreaterThan(0);
    expect(API_ENDPOINTS.every((endpoint) => ['Json', 'Png'].includes(endpoint.response_format))).toBe(
      true,
    );
    expect(API_ENDPOINTS.some((endpoint) => endpoint.path === API_PATHS.machineScreenshot)).toBe(true);
  });

  it('builds cache-busted screenshot URLs', () => {
    expect(machineScreenshotUrl(42, 'NativeFramebuffer')).toBe(
      '/api/machine/screenshot?t=42&kind=NativeFramebuffer',
    );
  });

  it('encodes query parameters and decodes successful responses', async () => {
    const response = {
      schema: 'rumiga.api.response.v1',
      version: 1,
      success: true,
      data: { path: '/disk image', files: [], total_bytes: 0, free_bytes: 1 },
      error: null,
      error_code: null,
    };
    const fetchMock = vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      new Response(JSON.stringify(response), { status: 200 }),
    );

    await expect(getFiles('/disk image')).resolves.toEqual(response);
    expect(fetchMock).toHaveBeenCalledWith('/api/files?path=%2Fdisk%20image', undefined);
  });

  it('sends JSON request bodies and reports API errors', async () => {
    const fetchMock = vi.spyOn(globalThis, 'fetch')
      .mockResolvedValueOnce(new Response('{}', { status: 200 }))
      .mockResolvedValueOnce(
        new Response(JSON.stringify({ error: 'Unavailable', error_code: 'E_TEST' }), { status: 503 }),
      );

    await expect(updateAudioSeparation({ separation: 50 })).resolves.toEqual({});
    const request = fetchMock.mock.calls[0]?.[1];
    expect(request?.method).toBe('POST');
    expect(request?.body).toBe(JSON.stringify({ separation: 50 }));

    await expect(getMachineStatus()).rejects.toThrow('Unavailable (E_TEST)');
  });

  it('covers the remaining client endpoints', async () => {
    const fetchMock = vi.spyOn(globalThis, 'fetch').mockImplementation(() =>
      Promise.resolve(new Response('{}', { status: 200 })),
    );
    const config = {} as Parameters<typeof updateMachineConfig>[0];

    await uploadFile(new File(['payload'], 'demo.adf'));
    await deleteFile('disk image.adf');
    await formatSd('confirm');
    await getWifiStatus();
    await scanWifi();
    await connectWifi('network', 'secret');
    await getMachineConfig();
    await updateMachineConfig(config);
    await startMachine();
    await stopMachine();
    await resetMachine();
    await pauseMachine();
    await resumeMachine();
    await insertFloppy({ drive_idx: 0, path: 'disk.adf' });
    await ejectFloppy({ drive_idx: 0 });
    await getSupportBundle();

    expect(fetchMock).toHaveBeenCalledTimes(16);
    expect(fetchMock.mock.calls.map(([url]) => url)).toEqual([
      API_PATHS.filesUpload,
      '/api/files/disk%20image.adf',
      API_PATHS.filesFormat,
      API_PATHS.wifiStatus,
      API_PATHS.wifiScan,
      API_PATHS.wifiConnect,
      API_PATHS.machineConfig,
      API_PATHS.machineConfig,
      API_PATHS.machineStart,
      API_PATHS.machineStop,
      API_PATHS.machineReset,
      API_PATHS.machinePause,
      API_PATHS.machineResume,
      API_PATHS.machineFloppyInsert,
      API_PATHS.machineFloppyEject,
      API_PATHS.machineSupportBundle,
    ]);
  });

  it('preserves plain-text HTTP errors', async () => {
    vi.spyOn(globalThis, 'fetch').mockResolvedValue(new Response('gateway failed', { status: 502 }));

    await expect(getMachineStatus()).rejects.toThrow('gateway failed');
  });
});

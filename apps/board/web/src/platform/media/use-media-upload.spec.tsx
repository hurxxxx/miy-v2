import { renderHook } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { useMediaUpload } from './use-media-upload';

const api = vi.fn();
const fileApi = vi.fn();

vi.mock('../runtime/runtime-context', () => ({
  useRuntime: () => ({ api, fileApi }),
}));

const reservation = { id: 'file-1', url: 'media:file-1', status: 'pending' };

describe('board image upload recovery', () => {
  beforeEach(() => {
    api.mockReset();
    fileApi.mockReset();
  });

  it('reuses the reservation request ID when its first response is lost', async () => {
    api.mockRejectedValueOnce(new TypeError('request response lost'));
    api.mockResolvedValueOnce(reservation);
    fileApi.mockResolvedValueOnce(new Response(null, { status: 200 }));
    const { result } = renderHook(useMediaUpload);
    const file = new File(['image'], 'image.png', { type: 'image/png' });

    expect(await result.current.uploadFile(file)).toBe(reservation.url);
    expect(api).toHaveBeenCalledTimes(2);
    expect(api.mock.calls[0]).toEqual(api.mock.calls[1]);
    expect(fileApi).toHaveBeenCalledTimes(1);
  });

  it('reuses the reserved file path when the transfer response is lost', async () => {
    api.mockResolvedValueOnce(reservation);
    fileApi.mockRejectedValueOnce(new TypeError('transfer response lost'));
    fileApi.mockResolvedValueOnce(new Response(null, { status: 200 }));
    const { result } = renderHook(useMediaUpload);
    const file = new File(['image'], 'image.png', { type: 'image/png' });

    expect(await result.current.uploadFile(file)).toBe(reservation.url);
    expect(api).toHaveBeenCalledTimes(1);
    expect(fileApi).toHaveBeenCalledTimes(2);
    expect(fileApi.mock.calls[0]).toEqual(fileApi.mock.calls[1]);
  });
});

import { describe, expect, it, vi } from 'vitest';
import { ApiError } from '@miy/sdk';
import { sendDownloadBatch, type DownloadBatch } from './file-downloads';

const batch: DownloadBatch = {
  remaining: [{ id: 'a', path: 'a.txt' }, { id: 'b', path: 'b.txt' }],
  total: 2,
  sent: 0,
};
const link = async (id: string) => ({ url: id, expires_at: '2026-11-22T15:00:00Z' });
type Download = Parameters<typeof sendDownloadBatch>[0];

describe('sendDownloadBatch', () => {
  it('waits for the first stream before starting the next file', async () => {
    let completeFirst!: () => void;
    const firstTransfer = new Promise<void>((resolve) => { completeFirst = resolve; });
    const download = vi.fn(async (url: string) => {
      if (url === 'a') await firstTransfer;
    });
    const getLink = vi.fn(link);
    const progress = vi.fn();
    const work = sendDownloadBatch(download, batch, getLink, new AbortController().signal, progress);
    await Promise.resolve();
    expect(getLink.mock.calls).toEqual([['a']]);
    expect(progress).toHaveBeenLastCalledWith({
      ...batch,
      remaining: [{ ...batch.remaining[0], downloadUrl: 'a' }, batch.remaining[1]],
    });
    completeFirst();
    await work;
    expect(getLink.mock.calls).toEqual([['a'], ['b']]);
    expect(progress).toHaveBeenLastCalledWith({ remaining: [], sent: 2, total: 2 });
  });

  it.each([
    new ApiError('http', 'file_download_failed'),
    new ApiError('http', 'not_found', 404),
  ])('replaces an unusable link after $code', async (failure) => {
    const download = vi.fn<Download>().mockResolvedValueOnce(undefined).mockRejectedValueOnce(failure);
    const getLink = vi.fn(link);
    let pending = batch;
    const progress = (next: DownloadBatch) => { pending = next; };
    const signal = new AbortController().signal;
    await expect(sendDownloadBatch(download, batch, getLink, signal, progress)).rejects.toThrow(failure.code);
    expect(pending).toEqual({ remaining: [{ id: 'b', path: 'b.txt' }], sent: 1, total: 2 });
    await sendDownloadBatch(download, pending, getLink, signal, progress);
    expect(getLink.mock.calls).toEqual([['a'], ['b'], ['b']]);
    expect(pending.sent).toBe(2);
  });

  it('checks the existing link after receipt loss without starting a second download', async () => {
    const download = vi.fn<Download>()
      .mockResolvedValueOnce(undefined)
      .mockRejectedValueOnce(new Error('receipt unavailable'))
      .mockResolvedValueOnce(undefined);
    const getLink = vi.fn(link);
    let pending = batch;
    const progress = (next: DownloadBatch) => { pending = next; };
    const signal = new AbortController().signal;

    await expect(sendDownloadBatch(download, batch, getLink, signal, progress)).rejects.toThrow('receipt unavailable');
    expect(pending).toEqual({
      remaining: [{ id: 'b', path: 'b.txt', downloadUrl: 'b' }],
      sent: 1,
      total: 2,
    });
    await sendDownloadBatch(download, pending, getLink, signal, progress);
    expect(getLink.mock.calls).toEqual([['a'], ['b']]);
    expect(download.mock.calls).toEqual([
      ['a', signal, { checkOnly: false }],
      ['b', signal, { checkOnly: false }],
      ['b', signal, { checkOnly: true }],
    ]);
    expect(pending.sent).toBe(2);
  });

  it('explicitly retries the blocked download on its existing link and continues the batch', async () => {
    const download = vi.fn<Download>()
      .mockRejectedValueOnce(new Error('file_download_start_unconfirmed'))
      .mockResolvedValue(undefined);
    const getLink = vi.fn(link);
    const signal = new AbortController().signal;
    let pending = batch;
    const progress = (next: DownloadBatch) => { pending = next; };

    await expect(sendDownloadBatch(download, batch, getLink, signal, progress)).rejects.toThrow('file_download_start_unconfirmed');
    await sendDownloadBatch(download, pending, getLink, signal, progress, true);

    expect(getLink.mock.calls).toEqual([['a'], ['b']]);
    expect(download.mock.calls).toEqual([
      ['a', signal, { checkOnly: false }],
      ['a', signal, { checkOnly: false }],
      ['b', signal, { checkOnly: false }],
    ]);
    expect(pending).toEqual({ remaining: [], sent: 2, total: 2 });
  });
});

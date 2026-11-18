import { api } from "./api";

/** Downloaded media for this session, keyed by chat and message id. */
const cache = $state<Record<string, string>>({});
const pending = new Map<string, Promise<string>>();

export function mediaKey(jid: string, id: string): string {
  return `${jid}\u0000${id}`;
}

export function cachedMedia(jid: string, id: string): string | undefined {
  return cache[mediaKey(jid, id)];
}

export function loadMedia(jid: string, id: string): Promise<string> {
  const key = mediaKey(jid, id);
  if (cache[key]) return Promise.resolve(cache[key]);
  let p = pending.get(key);
  if (!p) {
    p = api.media(jid, id).then((url) => {
      cache[key] = url;
      return url;
    });
    p.finally(() => pending.delete(key)).catch(() => {});
    pending.set(key, p);
  }
  return p;
}

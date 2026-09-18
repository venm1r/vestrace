import { useEffect, useRef, useState } from 'react';

const POLL_INTERVAL_MS = 2000;
const MAX_POLLS = 30;

export interface QualifiableItem {
  id: string;
  qualification_state: string | null;
}

/**
 * Re-fetches `list()` every `POLL_INTERVAL_MS` while `targetId` is set,
 * watching for that item's `qualification_state` to change from whatever it
 * was when polling started. Stops when it changes, after `MAX_POLLS`
 * attempts (one minute), when `targetId` becomes `null`, or on unmount.
 *
 * There is no per-qualification-job status route in this backend (see this
 * task's own notes) — the thing that changes observably over time is the
 * owning Connection/Model's list projection, once a qualification job
 * finishes and records a result against it.
 */
export function usePolledQualification<T extends QualifiableItem>(
  list: () => Promise<T[]>,
  targetId: string | null,
): { item: T | null; polling: boolean } {
  const [item, setItem] = useState<T | null>(null);
  const [polling, setPolling] = useState(false);
  const listRef = useRef(list);
  listRef.current = list;

  useEffect(() => {
    setItem(null);
    if (!targetId) {
      setPolling(false);
      return;
    }

    let active = true;
    let attempts = 0;
    let startingState: string | null | undefined;
    let timer: ReturnType<typeof setTimeout> | null = null;

    const poll = () => {
      setPolling(true);
      listRef
        .current()
        .then((items) => {
          if (!active) return;
          const found = items.find((candidate) => candidate.id === targetId) ?? null;
          setItem(found);
          if (startingState === undefined) startingState = found?.qualification_state ?? null;
          attempts += 1;
          const changed = found !== null && found.qualification_state !== startingState;
          if (changed || attempts >= MAX_POLLS) {
            setPolling(false);
          } else {
            timer = setTimeout(poll, POLL_INTERVAL_MS);
          }
        })
        .catch(() => {
          if (!active) return;
          attempts += 1;
          if (attempts >= MAX_POLLS) {
            setPolling(false);
          } else {
            // A transient read failure does not stop the poll; the next tick
            // tries again rather than freezing the UI on one bad response.
            timer = setTimeout(poll, POLL_INTERVAL_MS);
          }
        });
    };
    poll();

    return () => {
      active = false;
      if (timer) clearTimeout(timer);
    };
  }, [targetId]);

  return { item, polling };
}

/**
 * Retry helpers with exponential backoff used by the Acme platform services.
 *
 * The module is transport-agnostic: callers hand in a task closure and this
 * module decides how often and how patiently to rerun it.
 */

/** Options accepted by {@link retry}. */
export interface RetryOptions {
  /** Maximum number of attempts before giving up. Defaults to 3. */
  attempts?: number;
  /** Base delay in milliseconds. Doubles on every failed attempt. */
  baseDelayMs?: number;
  /** Return true when an error should not be retried. */
  isFatal?: (error: unknown) => boolean;
}

const sleep = (ms: number): Promise<void> =>
  new Promise((resolve) => setTimeout(resolve, ms));

/** Run `task` up to `attempts` times, waiting between failed attempts. */
export async function retry<T>(task: () => Promise<T>, options: RetryOptions = {}): Promise<T> {
  const attempts = options.attempts ?? 3;
  const baseDelayMs = options.baseDelayMs ?? 100;
  let lastError: unknown;

  for (let attempt = 1; attempt <= attempts; attempt += 1) {
    try {
      return await task();
    } catch (error) {
      lastError = error;
      if (options.isFatal?.(error) || attempt === attempts) {
        break;
      }
      await sleep(baseDelayMs * 2 ** (attempt - 1));
    }
  }

  throw lastError;
}

/** Format a duration in milliseconds as a short human-readable string. */
export function formatDuration(ms: number): string {
  if (ms < 1000) return `${ms}ms`;
  const seconds = ms / 1000;
  if (seconds < 60) return `${Number(seconds.toFixed(1))}s`;
  const minutes = Math.floor(seconds / 60);
  return `${minutes}m ${Math.round(seconds - minutes * 60)}s`;
}

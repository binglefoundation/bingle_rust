/**
 * The harness's state, kept outside the React tree (issue #431).
 *
 * Android recreates the activity for some configuration changes the app can't opt out of (an
 * overlay update changes the asset paths, `CONFIG_ASSETS_PATHS`). The JavaScript runtime survives,
 * but the root component remounts, so state held in it was lost: a command in flight resolved into
 * the unmounted component, and the new one showed `idle` until the test timed out. This module
 * lives as long as the runtime, so a remounted screen reads the same command, status, output and
 * events, and a command in flight still reports its result.
 */

export type CommandStatus = 'idle' | 'running' | 'ok' | 'error';

export interface HarnessState {
  command: string;
  status: CommandStatus;
  output: string;
  events: string[];
}

/** How many event-feed lines to keep. */
const MAX_EVENT_LINES = 300;

let state: HarnessState = {
  command: '{"method":"version","args":[]}',
  status: 'idle',
  output: '',
  events: [],
};

const listeners = new Set<() => void>();

/** The current state; a new object after every change, as `useSyncExternalStore` expects. */
export function getHarnessState(): HarnessState {
  return state;
}

/** Merge `changes` into the state and notify subscribers. */
export function updateHarnessState(changes: Partial<HarnessState>): void {
  state = {...state, ...changes};
  listeners.forEach(listener => listener());
}

/** Append a line to the event feed, keeping the last {@link MAX_EVENT_LINES}. */
export function pushEvent(line: string): void {
  updateHarnessState({events: [...state.events, line].slice(-MAX_EVENT_LINES)});
}

/** Call `listener` after every change; returns the unsubscribe function. */
export function subscribeHarnessState(listener: () => void): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

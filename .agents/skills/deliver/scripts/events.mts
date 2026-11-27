// Progress events in the agent-events core format (GravionLabs/agent-events, SPEC.md): the loop is the
// only writer, so `seq` is gapless. Displays such as the Deliver panel of poimen read the file; the
// session reports nothing. Writing never fails the caller.
import { appendFileSync, mkdirSync } from 'node:fs';
import path from 'node:path';

export type Tone = 'neutral' | 'info' | 'success' | 'warning' | 'danger';
export type Emit = (type: string, body?: Record<string, unknown>, tone?: Tone) => void;

/** `dir` is `<git-common-dir>/.agents/events`; the file is `<run>.jsonl`. */
export function openEvents(dir: string, run: string): Emit {
  let seq = 0;
  try {
    mkdirSync(dir, { recursive: true });
  } catch {
    // the first write reports nothing either
  }
  return (type, body = {}, tone = 'neutral') => {
    seq += 1;
    const event = { v: 1, seq, ts: new Date().toISOString(), run, type, tone, ...body };
    try {
      appendFileSync(path.join(dir, `${run}.jsonl`), `${JSON.stringify(event)}\n`);
    } catch {
      // a display must not stop a delivery
    }
  };
}

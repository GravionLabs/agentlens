// Starting and stopping the harness. On Windows the agent CLIs are `.cmd` shims (`claude.cmd`) that
// `spawn` without a shell does not find, and `child.kill()` ends only the shim, not the agent behind it.
import { type ChildProcess, execFileSync, spawn } from 'node:child_process';
import { existsSync } from 'node:fs';
import path from 'node:path';

/** A path as the session reads it in its prompt: forward slashes, quoted when it holds a space. */
export const promptPath = (file: string): string => {
  const slashed = file.replaceAll('\\', '/');
  return /\s/.test(slashed) ? `"${slashed}"` : slashed;
};

/** The file a command name stands for on Windows: PATH order, then PATHEXT order. */
export function resolveWindows(
  command: string,
  pathEnv: string,
  pathExt = '.COM;.EXE;.BAT;.CMD',
  exists: (file: string) => boolean = existsSync,
): string | undefined {
  const extensions = pathExt.split(';').filter(Boolean);
  const names = path.win32.extname(command)
    ? [command]
    : extensions.map((ext) => `${command}${ext}`);
  const dirs = path.win32.isAbsolute(command) ? [''] : pathEnv.split(';').filter(Boolean);
  for (const dir of dirs)
    for (const name of names) {
      const file = dir ? path.win32.join(dir, name) : name;
      if (exists(file)) return file;
    }
  return undefined;
}

/** The command line `cmd.exe /s /c` gets for a shim. Characters cmd expands inside quotes are refused. */
export function cmdLine(argv: string[]): string {
  const bad = argv.find((arg) => /[%\r\n\0]/.test(arg));
  if (bad !== undefined) throw new Error(`argument not usable with a .cmd shim: ${bad}`);
  return `"${argv.map((arg) => `"${arg.replaceAll('"', '""')}"`).join(' ')}"`;
}

/** The program and arguments that start `argv` on this platform. */
export function launchCommand(
  argv: string[],
  platform: NodeJS.Platform = process.platform,
  env: NodeJS.ProcessEnv = process.env,
): { file: string; args: string[]; verbatim: boolean } {
  if (platform !== 'win32') return { file: argv[0], args: argv.slice(1), verbatim: false };
  const found = resolveWindows(argv[0], env.PATH ?? env.Path ?? '', env.PATHEXT);
  if (!found || !/\.(cmd|bat)$/i.test(found))
    return { file: found ?? argv[0], args: argv.slice(1), verbatim: false };
  return {
    file: env.ComSpec ?? 'cmd.exe',
    args: ['/d', '/s', '/c', cmdLine([found, ...argv.slice(1)])],
    verbatim: true,
  };
}

export function launch(argv: string[], options: Parameters<typeof spawn>[2]): ChildProcess {
  const { file, args, verbatim } = launchCommand(argv, process.platform, options?.env);
  return spawn(file, args, { ...options, windowsVerbatimArguments: verbatim });
}

/** Ends the process and, on Windows, everything it started. */
export function killTree(child: ChildProcess): void {
  if (process.platform === 'win32' && child.pid !== undefined) {
    try {
      execFileSync('taskkill', ['/pid', String(child.pid), '/T', '/F'], { stdio: 'ignore' });
      return;
    } catch {
      // already gone, or taskkill failed: fall through to the plain kill
    }
  }
  child.kill();
}

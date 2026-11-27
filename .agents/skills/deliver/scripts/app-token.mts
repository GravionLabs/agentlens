// Installation tokens of a GitHub App for the sessions of the loop. A token is limited to one repository,
// carries only the permissions asked for and expires after one hour. The private key is read from a file
// or from a command of the user's environment; it is used to sign one JWT and is never printed or stored.
import { execSync } from 'node:child_process';
import { createSign } from 'node:crypto';
import { readFileSync } from 'node:fs';
import { adoptLegacyEnv } from './legacy.mts';

/** Contents, pull requests and issues. No `workflows`, no `administration`. */
export const APP_PERMISSIONS = { contents: 'write', pull_requests: 'write', issues: 'write' };

export type AppSettings = { appId: string; keyFile?: string; keyCommand?: string };
export type AppToken = { token: string; expiresAt: number };

/** The app settings of the environment, else nothing. A half-set configuration is an error. */
export function appSettings(source: NodeJS.ProcessEnv = process.env): AppSettings | undefined {
  const env = { ...source };
  adoptLegacyEnv(env);
  const appId = env.DELIVER_APP_ID;
  const keyFile = env.DELIVER_APP_KEY_FILE;
  const keyCommand = env.DELIVER_APP_KEY_COMMAND;
  if (!appId && !keyFile && !keyCommand) return undefined;
  if (!appId) throw new Error('DELIVER_APP_ID is missing');
  if (!keyFile === !keyCommand)
    throw new Error('set exactly one of DELIVER_APP_KEY_FILE or DELIVER_APP_KEY_COMMAND');
  return { appId, keyFile, keyCommand };
}

const base64url = (value: Buffer | string): string => Buffer.from(value).toString('base64url');

/** The JWT that authenticates as the app (RS256, valid for 9 minutes, issued 60 s in the past for clock skew). */
export function appJwt(appId: string, privateKey: string, now = Date.now()): string {
  const seconds = Math.floor(now / 1000);
  const head = base64url(JSON.stringify({ alg: 'RS256', typ: 'JWT' }));
  const claims = base64url(JSON.stringify({ iat: seconds - 60, exp: seconds + 540, iss: appId }));
  const signature = createSign('RSA-SHA256').update(`${head}.${claims}`).sign(privateKey);
  return `${head}.${claims}.${base64url(signature)}`;
}

function readKey(settings: AppSettings): string {
  const key = settings.keyFile
    ? readFileSync(settings.keyFile, 'utf8')
    : execSync(settings.keyCommand!, {
        encoding: 'utf8',
        stdio: ['ignore', 'pipe', 'pipe'],
        maxBuffer: 1024 * 1024,
      });
  if (!key.includes('PRIVATE KEY')) throw new Error('the app key is not a PEM private key');
  return key;
}

/** Mints an installation token for `repo` (`owner/name`). Errors never contain the key or a token. */
export async function mintToken(
  settings: AppSettings,
  repo: string,
  apiUrl = process.env.DELIVER_APP_API_URL ?? 'https://api.github.com',
): Promise<AppToken> {
  let jwt: string;
  try {
    jwt = appJwt(settings.appId, readKey(settings));
  } catch (error) {
    // the message of a failed command can quote its output: say only what failed
    throw new Error(
      settings.keyFile
        ? `cannot sign with the app key file: ${(error as NodeJS.ErrnoException).code ?? 'invalid key'}`
        : 'the app key command failed or gave no usable key',
    );
  }
  const call = async (method: string, route: string, body?: unknown) => {
    const response = await fetch(`${apiUrl}${route}`, {
      method,
      headers: {
        Authorization: `Bearer ${jwt}`,
        Accept: 'application/vnd.github+json',
        'X-GitHub-Api-Version': '2022-11-28',
        ...(body ? { 'Content-Type': 'application/json' } : {}),
      },
      body: body ? JSON.stringify(body) : undefined,
    });
    if (!response.ok) throw new Error(`GitHub ${method} ${route}: HTTP ${response.status}`);
    return (await response.json()) as Record<string, unknown>;
  };
  const installation = await call('GET', `/repos/${repo}/installation`);
  const created = await call('POST', `/app/installations/${installation.id}/access_tokens`, {
    repositories: [repo.split('/')[1]],
    permissions: APP_PERMISSIONS,
  });
  return { token: String(created.token), expiresAt: Date.parse(String(created.expires_at)) };
}

/** Hands out a token and mints a new one when less than `minimumMs` of the current one is left. */
export function tokenSource(
  settings: AppSettings,
  repo: string,
  minimumMs: number,
  mint: typeof mintToken = mintToken,
): { current(now?: number): Promise<{ token: string; renewed: boolean }> } {
  let held: AppToken | undefined;
  return {
    async current(now = Date.now()) {
      if (held && held.expiresAt - now >= minimumMs) return { token: held.token, renewed: false };
      held = await mint(settings, repo);
      return { token: held.token, renewed: true };
    },
  };
}

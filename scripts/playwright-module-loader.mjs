import { accessSync, constants, statSync } from 'node:fs';
import { createRequire } from 'node:module';
import { join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const pinnedModule = fileURLToPath(new URL('./ci-browser-tools/node_modules/playwright', import.meta.url));

// Use the explicit legacy override first, then the verified CI export, the
// repository-pinned tools, and finally the invoking script’s local installation.
export function loadPlaywright(callerUrl, env = process.env) {
  const require = createRequire(callerUrl);
  const explicit = env.OASIS7_PLAYWRIGHT_NODE_MODULES
    ? ['OASIS7_PLAYWRIGHT_NODE_MODULES', join(resolve(env.OASIS7_PLAYWRIGHT_NODE_MODULES), 'playwright')]
    : env.OASIS7_PLAYWRIGHT_MODULE
      ? ['OASIS7_PLAYWRIGHT_MODULE', resolve(env.OASIS7_PLAYWRIGHT_MODULE)]
      : null;

  if (explicit) {
    let entry;
    try {
      entry = require.resolve(explicit[1]);
    } catch (cause) {
      throw new Error(explicit[0] + ' must select a Playwright installation', { cause });
    }
    return require(entry);
  }
  for (const name of [pinnedModule, 'playwright']) {
    let entry;
    try {
      entry = require.resolve(name);
    } catch (error) {
      if (error?.code === 'MODULE_NOT_FOUND') continue;
      throw error;
    }
    return require(entry);
  }
  throw new Error('Playwright is not available; install the pinned browser tools or set OASIS7_PLAYWRIGHT_MODULE');
}

export function browserExecutablePath(requestedPath = process.env.OASIS7_PLAYWRIGHT_CHROME_PATH) {
  if (!requestedPath) return undefined;
  const path = resolve(requestedPath);
  try {
    if (!statSync(path).isFile()) throw new Error('not a file');
    accessSync(path, constants.X_OK);
    return path;
  } catch (cause) {
    throw new Error('OASIS7_PLAYWRIGHT_CHROME_PATH or --chrome-path must select an executable file', { cause });
  }
}

// Persistent profile and passkey probes need the full browser, not headless shell.
export function fullBrowserLaunchOptions(executablePath) {
  return executablePath === undefined ? { channel: 'chromium' } : { executablePath };
}

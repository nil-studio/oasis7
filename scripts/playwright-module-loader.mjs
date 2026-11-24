import { createRequire } from 'node:module';
import { existsSync } from 'node:fs';
import { join, resolve } from 'node:path';

// Resolve from the actual caller by default, not a particular Coding Agent's
// private runtime. A user-supplied node_modules directory takes precedence.
export function loadPlaywright(callerUrl) {
  const override = process.env.OASIS7_PLAYWRIGHT_NODE_MODULES;
  if (override && !existsSync(join(resolve(override), 'playwright'))) {
    throw new Error('OASIS7_PLAYWRIGHT_NODE_MODULES must contain a playwright package');
  }
  const identifier = override ? join(resolve(override), 'playwright') : 'playwright';
  try {
    return createRequire(callerUrl)(identifier);
  } catch (error) {
    if (error?.code !== 'MODULE_NOT_FOUND' ||
        !String(error.message).includes(`Cannot find module '${identifier}'`)) {
      throw error;
    }
    throw new Error(
      'Playwright is not available; install it near the calling script or set ' +
      'OASIS7_PLAYWRIGHT_NODE_MODULES to its node_modules directory',
      { cause: error },
    );
  }
}

import test from 'node:test';
import assert from 'node:assert/strict';
import { verifyStartupCacheCases } from './viewer-startup-cache-policy.mjs';
const sample = () => [{ profile: 'A', networkRequests: 1 }, { profile: 'A', networkRequests: 0 }, { profile: 'B', networkRequests: 1 }].map((x, i) => ({ ...x, wasmCompiled: true, responses: [{ status: 200, fromDiskCache: i === 1, fromServiceWorker: false }] }));
test('separate cold profiles and same-profile disk-cache warm capability', () => assert.equal(verifyStartupCacheCases(sample()).startupBudget, 'unmeasured'));
test('fresh profile must not inherit cached response', () => { const x = sample(); x[2].responses[0].fromDiskCache = true; assert.throws(() => verifyStartupCacheCases(x)); });
test('warm network fallback cannot be labelled disk-cache reuse', () => { const x = sample(); x[1].networkRequests = 1; assert.throws(() => verifyStartupCacheCases(x)); });
test('service worker, missing compile and shared cold profile fail closed', () => { for (const change of [x => x[1].responses[0].fromServiceWorker = true, x => x[0].wasmCompiled = false, x => x[2].profile = 'A']) { const x = sample(); change(x); assert.throws(() => verifyStartupCacheCases(x)); } });

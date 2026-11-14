import { it, expect, afterEach } from 'vitest';
import { renderPixelWorldHost, sampleSnapshot, cleanupMountedHost, useTestRustRenderState } from './pixel_world_host_test_support.jsx';
afterEach(()=>{cleanupMountedHost();delete window.__OASIS7_WORLD_VIEW_RETENTION__;});
it('installs only a gated read and removes its own instance on cleanup',async()=>{
 useTestRustRenderState();await renderPixelWorldHost(sampleSnapshot(),'?test_api=1&connect=0&locale=en');
 expect(typeof window.__OASIS7_WORLD_VIEW_RETENTION__).toBe('function');
 const stats=window.__OASIS7_WORLD_VIEW_RETENTION__();expect(stats.published).toHaveProperty('logicalJsonBytes');
 expect(stats.transport.pendingRequests).toBe(0);
 expect(JSON.stringify(stats)).not.toContain('agent-0');expect(window.__OASIS7_WORLD_VIEW_RETENTION__(null,false).measured).toBe(false);
 cleanupMountedHost();expect(window.__OASIS7_WORLD_VIEW_RETENTION__).toBeUndefined();
});
it('does not delete another mounted instance getter or expose the hook without test_api',async()=>{
 useTestRustRenderState();await renderPixelWorldHost(sampleSnapshot(),'?test_api=1&connect=0&locale=en');
 const other=()=>null;window.__OASIS7_WORLD_VIEW_RETENTION__=other;cleanupMountedHost();expect(window.__OASIS7_WORLD_VIEW_RETENTION__).toBe(other);
 delete window.__OASIS7_WORLD_VIEW_RETENTION__;
 await renderPixelWorldHost(sampleSnapshot(),'?connect=0&locale=en','en',{injectionSearch:'?test_api=1&connect=0&locale=en'});expect(window.__OASIS7_WORLD_VIEW_RETENTION__).toBeUndefined();
});

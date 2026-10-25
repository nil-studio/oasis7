// Test-only binding access; never a production ingress or authorization shortcut.
export { RecoveryFence } from '../../artifacts/recovery-fence/index.js';
export default { fetch() { return new Response(null, { status: 404 }); } };

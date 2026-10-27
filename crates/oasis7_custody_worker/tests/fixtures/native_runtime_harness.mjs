// Fixture-only manifest lifecycle access; no Owner, native HTTP or signing binding.
export { NativeSession } from '../../artifacts/native-harness/index.js';
export default { fetch() { return new Response(null, { status: 404 }); } };

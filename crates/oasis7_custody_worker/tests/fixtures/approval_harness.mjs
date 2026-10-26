// Component-only binding access. No production ingress imports this artifact.
export { TestApprovalAuthority } from '../../artifacts/approval-harness/index.js';
export default { fetch() { return new Response(null, { status: 404 }); } };

// Typecheck the actual HTTP consumer against the Rust-generated declarations.
import type { ServiceMetadata, ErrorResponse } from '../contract/http';
export function stateText(value: ServiceMetadata): string {
  return `${value.role}: ${value.execution_enabled ? 'enabled' : 'not ready'}`;
}
export function errorText(value: ErrorResponse): string { return value.code; }
const example: ServiceMetadata = { role: 'owner', execution_enabled: false,
  schema_revision: 'custody-http-v1', api_profile: 'custody-foundation-v1' };
stateText(example);
// @ts-expect-error protocol keys are not public metadata
stateText({ ...example, private_key: 'never' });
// @ts-expect-error unknown roles are rejected
stateText({ ...example, role: 'admin' });

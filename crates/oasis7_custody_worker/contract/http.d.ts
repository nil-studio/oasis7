// Generated from actual Rust handler DTOs. Do not edit.
export type Role = "identity" | "owner" | "executor" | "recovery-fence";
export type ErrorCode = "not_found" | "not_ready" | "method_not_allowed";
export type ServiceMetadata = {
  "api_profile": string;
  "execution_enabled": boolean;
  "role": Role;
  "schema_revision": string;
};
export type ErrorResponse = {
  "code": ErrorCode;
  "schema_revision": string;
};

-- Apply only through an independent provisioning identity. Runtime never runs DDL.
-- This schema supplies SQL atomicity, not HSM custody or rollback protection.
CREATE TABLE oasis7_bft_v3_namespaces (
    namespace TEXT PRIMARY KEY CHECK (length(namespace) BETWEEN 1 AND 128),
    binding_digest TEXT NOT NULL CHECK (binding_digest ~ '^[0-9a-f]{64}$'),
    body BYTEA NOT NULL CHECK (octet_length(body) BETWEEN 1 AND 536870912)
);

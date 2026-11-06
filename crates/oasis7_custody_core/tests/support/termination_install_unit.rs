// Unit-only wrapper, preserving the exact external fixture module hierarchy.
macro_rules! termination_install_fixture_extension {
    (authority) => { mod account_install_actual_state_tests {
        include!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/support/account_install_actual_state_tests.rs"));
    } };
    (legacy) => { include!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/support/account_install_old_allocation_test.rs")); };
}
include!("approval_admission_shared.rs");

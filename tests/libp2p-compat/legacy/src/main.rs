mod adapter;

fn main() {
    oasis7_libp2p_compat_protocol::main::<adapter::Network>();
}

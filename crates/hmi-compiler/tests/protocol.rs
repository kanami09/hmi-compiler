mod protocol {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/support/cli_protocol.rs"
    ));
}
#[test]
fn compiler_uses_the_shared_cli_protocol() {
    protocol::assert_protocol(env!("CARGO_BIN_EXE_hmi-compile"), "hmi-compile");
}

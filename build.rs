fn main() {
    // The icon, version info, and manifest are Windows resources. `cfg(windows)` describes the
    // build host, which is where the resource compiler runs; the environment variable describes
    // the target.
    #[cfg(windows)]
    if std::env::var_os("CARGO_CFG_WINDOWS").is_some() {
        embed_resource::compile("vsrecent.rc", embed_resource::NONE)
            .manifest_required()
            .unwrap();
    }
}

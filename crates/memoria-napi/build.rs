fn main() {
    if std::env::var_os("CARGO_FEATURE_NATIVE_LINK").is_some() {
        napi_build::setup();
    }
}

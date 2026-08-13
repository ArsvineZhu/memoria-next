use memoria_runtime::RuntimeError;

pub fn to_napi_error(error: impl std::fmt::Display) -> napi::Error {
    napi::Error::from_reason(format!("NATIVE_ERROR: {error}"))
}

pub fn runtime_error(error: RuntimeError) -> napi::Error {
    napi::Error::from_reason(format!("{}: {error}", error.code()))
}

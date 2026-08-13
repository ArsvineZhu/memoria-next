use memoria_runtime::RuntimeError;

pub fn to_napi_error(error: impl std::fmt::Display) -> napi::Error {
    napi::Error::from_reason(error.to_string())
}

pub fn runtime_error(error: RuntimeError) -> napi::Error {
    to_napi_error(error)
}

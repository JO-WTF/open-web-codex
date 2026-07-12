#[cfg_attr(any(target_os = "ios", target_os = "android", not(feature = "app-runtime")), path = "stub.rs")]
#[cfg_attr(all(not(any(target_os = "ios", target_os = "android")), feature = "app-runtime"), path = "real.rs")]
mod imp;

pub(crate) use imp::*;

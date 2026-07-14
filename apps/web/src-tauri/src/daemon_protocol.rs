/// Increment whenever the daemon RPC method set or payload contract changes.
///
/// This is intentionally independent from the application package version so
/// development builds with the same package version do not reuse an
/// incompatible daemon process.
pub(crate) const DAEMON_RPC_REVISION: u32 = 2;

/// Security combinations currently supported by the connection flow.
/// Credentials are stored separately so clearing them preserves the required mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ConnectionSecurity {
    SaslSslScramSha512,
}

use crate::{enums::AuthenticationError, imports::jaas, models::ScramCredentials};

pub(super) const SUPPORTED_PROTOCOLS: &[&str] = &["SASL_SSL"];
pub(super) const SASL_MECHANISM: &[&str] = &["SCRAM-SHA-512"];

/// 校验当前支持的认证组合，并解析已完成 XML 解码的 JAAS 文本。
pub(super) fn parse_authentication(
    security_protocol: &str,
    sasl_mechanism: &str,
    jaas: &str,
) -> Result<ScramCredentials, AuthenticationError> {
    if !SUPPORTED_PROTOCOLS
        .iter()
        .any(|e| e.eq_ignore_ascii_case(security_protocol.trim()))
    {
        return Err(AuthenticationError::UnsupportedSecurityProtocol);
    }
    if !SASL_MECHANISM
        .iter()
        .any(|e| e.eq_ignore_ascii_case(sasl_mechanism.trim()))
    {
        return Err(AuthenticationError::UnsupportedSaslMechanism);
    }
    let credentials = match jaas::parse_scram_jaas(jaas) {
        Ok(credentials) => credentials,
        Err(e) => return Err(AuthenticationError::Jaas(e)),
    };
    Ok(credentials)
}

#[cfg(test)]
mod tests {
    use super::parse_authentication;
    use crate::enums::{AuthenticationError, JaasError};

    const JAAS: &str = "org.apache.kafka.common.security.scram.ScramLoginModule required username='Demo' password=' Pass Word ';";

    #[test]
    fn accepts_supported_authentication() -> Result<(), AuthenticationError> {
        for (protocol, mechanism) in [
            ("SASL_SSL", "SCRAM-SHA-512"),
            (" sasl_ssl\t", " scram-sha-512 "),
        ] {
            let credentials = parse_authentication(protocol, mechanism, JAAS)?;

            // 协议名称可以归一化，凭据的大小写与空白必须保持原样。
            assert_eq!(credentials.username, "Demo");
            assert_eq!(credentials.password.expose(), " Pass Word ");
        }
        Ok(())
    }

    #[test]
    fn rejects_unsupported_configuration_and_preserves_jaas_error() {
        let cases = [
            (
                "SSL",
                "SCRAM-SHA-512",
                JAAS,
                AuthenticationError::UnsupportedSecurityProtocol,
            ),
            (
                "SASL_SSL",
                "PLAIN",
                JAAS,
                AuthenticationError::UnsupportedSaslMechanism,
            ),
            (
                "SASL_SSL",
                "SCRAM-SHA-512",
                "org.apache.kafka.common.security.scram.ScramLoginModule required username='Demo';",
                AuthenticationError::Jaas(JaasError::MissingPassword),
            ),
        ];
        for (protocol, mechanism, jaas, expected) in cases {
            match parse_authentication(protocol, mechanism, jaas) {
                Err(actual) => assert_eq!(actual, expected),
                Ok(_) => panic!("Expected invalid authentication to be rejected"),
            }
        }
    }
}

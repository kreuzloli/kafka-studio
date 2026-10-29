/// 包装内存中的秘密文本，避免通过 Debug 意外输出原文。
pub(crate) struct SecretString(String);

impl SecretString {
    pub(crate) fn new(value: String) -> Self {
        Self(value)
    }

    pub(crate) fn expose(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for SecretString {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[REDACTED]")
    }
}

#[cfg(test)]
mod tests {
    use crate::models::{ScramCredentials, SecretString};

    #[test]
    fn debug_test() {
        let secret = SecretString::new("DEMO_ONLY".to_owned());
        assert_eq!(secret.expose(), "DEMO_ONLY");
        assert_eq!(format!("{secret:?}"), "[REDACTED]");
    }

    #[test]
    fn credentials_debug_test() {
        let credentials = ScramCredentials::new("root".to_string(), "123456".to_string());
        assert_eq!(format!("{credentials:?}"), "ScramCredentials([REDACTED])");
        assert_eq!(format!("{credentials:#?}"), "ScramCredentials([REDACTED])");
    }
}

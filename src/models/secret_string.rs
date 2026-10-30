/// 包装内存中的秘密文本，避免通过 Debug 意外输出原文。
pub(crate) struct SecretString(String);

impl SecretString {
    /// 接管秘密文本；仅提供 Debug 脱敏，不保证释放时清零内存。
    pub(crate) fn new(value: String) -> Self {
        Self(value)
    }

    /// 临时借用原文供认证或钥匙串调用；返回值不得写入日志。
    pub(crate) fn expose(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for SecretString {
    /// 所有 Debug 格式均只显示固定占位符。
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

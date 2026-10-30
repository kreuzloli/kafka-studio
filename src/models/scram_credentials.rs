use crate::models::secret_string::SecretString;

/// JAAS 解析得到的凭据，仅用于内存中的认证处理，不保存原始配置文本。
pub(crate) struct ScramCredentials {
    pub(crate) username: String,
    pub(crate) password: SecretString,
}

impl ScramCredentials {
    /// 保留用户名和密码的原始空白与大小写，不对凭据做归一化。
    pub(crate) fn new(username: String, password: String) -> Self {
        ScramCredentials {
            username,
            password: SecretString::new(password),
        }
    }
}

impl std::fmt::Debug for ScramCredentials {
    /// 整体隐藏凭据，避免派生 Debug 暴露用户名。
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ScramCredentials([REDACTED])")
    }
}

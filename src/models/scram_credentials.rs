/// JAAS 解析得到的凭据，仅用于内存中的认证处理，不保存原始配置文本。
pub(crate) struct ScramCredentials {
    pub(crate) username: String,
    pub(crate) password: String,
}

impl ScramCredentials {
    pub(crate) fn new(username: String, password: String) -> Self {
        ScramCredentials { username, password }
    }
}

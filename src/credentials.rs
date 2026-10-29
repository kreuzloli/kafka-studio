/// 使用稳定的连接 ID 定位系统凭据，避免连接重名导致密码覆盖。
fn entry_for(connection_id: &str) -> Result<keyring::Entry, keyring::Error> {
    keyring::Entry::new("kafka-studio", connection_id)
}

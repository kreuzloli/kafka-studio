//! 系统钥匙串适配；日志仅记录连接 ID 和操作结果，不输出凭据或底层错误原文。

use crate::models::{Connection, ScramCredentials, SecretString};

/// 使用稳定的连接 ID 定位系统凭据，避免连接重名导致密码覆盖。
fn entry_for(connection_id: &str) -> Result<keyring::Entry, keyring::Error> {
    keyring::Entry::new("kafka-studio", connection_id).inspect_err(|_| {
        eprintln!("[kafka-studio] event=credential_entry_failed connection_id={connection_id}");
    })
}

/// 将连接密码保存到系统凭据存储；存储失败时向调用方返回错误。
pub(crate) fn save_password(
    connection_id: &str,
    password: &SecretString,
) -> Result<(), keyring::Error> {
    let entry = entry_for(connection_id)?;
    entry.set_password(password.expose()).inspect_err(|_| {
        eprintln!("[kafka-studio] event=credential_save_failed connection_id={connection_id}");
    })?;
    eprintln!("[kafka-studio] event=credential_saved connection_id={connection_id}");
    Ok(())
}

/// 读取连接密码；没有保存过时返回 None，其他存储错误向上传递。
pub(crate) fn load_password(connection_id: &str) -> Result<Option<SecretString>, keyring::Error> {
    let entry = entry_for(connection_id)?;
    match entry.get_password() {
        Ok(password) => {
            eprintln!("[kafka-studio] event=credential_loaded connection_id={connection_id}");
            Ok(Some(SecretString::new(password)))
        }
        Err(keyring::Error::NoEntry) => {
            eprintln!("[kafka-studio] event=credential_not_found connection_id={connection_id}");
            Ok(None::<SecretString>)
        }
        Err(e) => {
            eprintln!("[kafka-studio] event=credential_load_failed connection_id={connection_id}");
            Err(e)
        }
    }
}

/// 删除连接的已保存密码；条目不存在时也视为成功。
pub(crate) fn delete_password(connection_id: &str) -> Result<(), keyring::Error> {
    let entry = entry_for(connection_id)?;
    match entry.delete_credential() {
        Ok(_) | Err(keyring::Error::NoEntry) => {
            // 删除的目标是条目不存在，因此重复删除也视为完成。
            eprintln!("[kafka-studio] event=credential_removed connection_id={connection_id}");
            Ok(())
        }
        Err(e) => {
            eprintln!(
                "[kafka-studio] event=credential_delete_failed connection_id={connection_id}"
            );
            Err(e)
        }
    }
}

/// 保存连接当前持有的密码；没有会话凭据时不执行保存。
pub(crate) fn remember_password(connection: &Connection) -> Result<(), keyring::Error> {
    if let Some(credentials) = &connection.credentials {
        let connection_id = connection.id.as_str();
        let password = &credentials.password;
        return save_password(connection_id, password);
    }
    Ok(())
}

/// 读取已保存的密码，并结合调用方提供的用户名恢复凭据。
pub(crate) fn restore_credentials(
    connection_id: &str,
    username: String,
) -> Result<Option<ScramCredentials>, keyring::Error> {
    // 钥匙串只保存密码；用户名由调用方维护，避免形成两份配置来源。
    let password = load_password(connection_id)?;
    match password {
        Some(password) => Ok(Some(ScramCredentials { username, password })),
        None => Ok(None::<ScramCredentials>),
    }
}
#[cfg(test)]
mod tests {

    use crate::{
        credentials::{delete_password, remember_password, restore_credentials},
        models::{Connection, ConnectionFolderId, ScramCredentials, SecretString},
    };

    // 会访问本机钥匙串，需显式运行；随机 ID 避免覆盖已有连接的密码。
    #[test]
    #[ignore]
    fn test_keyring() -> Result<(), keyring::Error> {
        let username = String::from("root");
        let mut conn = Connection {
            id: uuid::Uuid::new_v4().to_string(),
            name: String::from("test-conn"),
            bootstrap_servers: String::from("test-server"),
            folder_id: Some(ConnectionFolderId(String::from("folder-1"))),
            credentials: Some(ScramCredentials {
                username: username.clone(),
                password: SecretString::new(String::from("123456")),
            }),
        };
        remember_password(&conn)?;
        conn.clear_credentials();
        conn.credentials = restore_credentials(&conn.id, username)?;

        let credentials = conn
            .credentials
            .as_ref()
            .expect("Expected restored credentials");
        assert_eq!(credentials.password.expose(), "123456");
        delete_password(&conn.id)?;
        Ok(())
    }
}

use crate::models::{ConnectionFolderId, ScramCredentials};

/// 本地连接配置；保存名称和地址不代表已经建立网络连接。
#[derive(Debug)]
pub(crate) struct Connection {
    pub(crate) name: String,
    pub(crate) bootstrap_servers: String,
    /// 所属目录；None 表示连接直接位于根层级。
    pub(crate) folder_id: Option<ConnectionFolderId>,
    /// 当前会话使用的凭据；不得作为普通连接配置明文持久化。
    pub(crate) credentials: Option<ScramCredentials>,
}

impl Connection {
    /// 检查单个主机名或 IPv4 加端口的基本结构，不解析 DNS、不建立连接。
    pub(crate) fn validate_bootstrap_server(value: &str) -> Result<(), &'static str> {
        let Some((host, port)) = value.split_once(':') else {
            return Err("Use host:port, for example localhost:9092");
        };
        if host.is_empty() || value.chars().any(char::is_whitespace) {
            return Err("Host cannot be empty and address cannot contain whitespace");
        }
        if value.contains(',') || port.contains(':') {
            return Err("Only one host:port is supported for now; IPv6 is not supported yet");
        }
        if !host
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '-' | '_'))
        {
            return Err("Host contains unsupported characters");
        }
        match port.parse::<u16>() {
            Ok(port) if port > 0 => Ok(()),
            _ => Err("Port must be a number from 1 to 65535"),
        }
    }

    /// 逐项校验地址并去除各项首尾空白，返回逗号连接的规范化字符串。
    pub(crate) fn normalize_bootstrap_servers(value: &str) -> Result<String, &'static str> {
        let mut servers = Vec::<&str>::new();
        for server in value.split(',') {
            let server = server.trim();
            if server.is_empty() {
                return Err("Bootstrap servers cannot contain an empty entry");
            }
            Self::validate_bootstrap_server(server)?;
            servers.push(server);
        }
        Ok(servers.join(","))
    }
}

#[cfg(test)]
mod tests {
    use super::Connection;

    #[test]
    fn normalizes_multiple_servers() {
        let result = Connection::normalize_bootstrap_servers(" localhost:9092, test.example:9093 ");

        assert_eq!(result, Ok("localhost:9092,test.example:9093".to_owned()));
    }

    #[test]
    fn rejects_empty_server() {
        for value in ["", "localhost:9092,", "localhost:9092,,test.example:9093"] {
            assert_eq!(
                Connection::normalize_bootstrap_servers(value),
                Err("Bootstrap servers cannot contain an empty entry")
            );
        }
    }

    #[test]
    fn rejects_invalid_port_in_server_list() {
        for value in [
            "localhost:9092,test.example:0",
            "localhost:9092,test.example:65536",
            "localhost:9092,test.example:abc",
        ] {
            assert_eq!(
                Connection::normalize_bootstrap_servers(value),
                Err("Port must be a number from 1 to 65535")
            );
        }
    }
}

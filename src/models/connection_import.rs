use crate::models::ScramCredentials;

/// XML 中的目录记录；ID 只在本次导入文件内有意义。
pub(crate) struct ImportedFolder {
    pub(crate) external_id: String,
    pub(crate) name: String,
    pub(crate) parent_external_id: Option<String>,
}

impl ImportedFolder {
    pub(crate) fn new() -> Self {
        Self {
            external_id: String::new(),
            name: String::new(),
            parent_external_id: None,
        }
    }
}

/// 当前结构解析阶段读取的连接字段，尚未转换为应用内配置。
pub(crate) struct ImportedConnection {
    pub(crate) name: String,
    pub(crate) bootstrap_servers: String,
    pub(crate) folder_external_id: Option<String>,
    /// XML 的 group 属性，用于检查名称提示与目录引用是否一致。
    pub(crate) folder_name_hint: Option<String>,
    /// 已通过当前认证规则校验的凭据；None 表示未提供认证配置。
    pub(crate) credentials: Option<ScramCredentials>,
}

impl ImportedConnection {
    pub(crate) fn new() -> Self {
        Self {
            name: String::new(),
            bootstrap_servers: String::new(),
            folder_external_id: None,
            folder_name_hint: None,
            credentials: None,
        }
    }
}

/// 一份 XML 的结构解析结果，供后续统一校验和转换。
pub(crate) struct ConnectionImport {
    pub(crate) folders: Vec<ImportedFolder>,
    pub(crate) connections: Vec<ImportedConnection>,
}

impl ConnectionImport {
    pub(crate) fn new() -> Self {
        Self {
            folders: Vec::<ImportedFolder>::new(),
            connections: Vec::<ImportedConnection>::new(),
        }
    }
}

use crate::models::ScramCredentials;

/// XML 中的目录记录；ID 只在本次导入文件内有意义。
pub(crate) struct ImportedFolder {
    pub(crate) external_id: String,
    pub(crate) name: String,
    pub(crate) parent_external_id: Option<String>,
}

impl ImportedFolder {
    /// 创建待解析的目录记录；必填字段由解析器填充并校验。
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
    /// XML 中的原始信任库路径；导入后仍需确认或映射到本机资源。
    pub(crate) truststore_location: Option<String>,
}

impl ImportedConnection {
    /// 创建待解析的连接记录；缺省凭据不代表已经通过认证。
    pub(crate) fn new() -> Self {
        Self {
            name: String::new(),
            bootstrap_servers: String::new(),
            folder_external_id: None,
            folder_name_hint: None,
            credentials: None,
            truststore_location: None,
        }
    }
}

/// 一份 XML 的结构解析结果，供后续统一校验和转换。
pub(crate) struct ConnectionImport {
    pub(crate) folders: Vec<ImportedFolder>,
    pub(crate) connections: Vec<ImportedConnection>,
}

impl ConnectionImport {
    /// 创建一份独立的导入结果，收集连接及目录后再统一转换。
    pub(crate) fn new() -> Self {
        Self {
            folders: Vec::<ImportedFolder>::new(),
            connections: Vec::<ImportedConnection>::new(),
        }
    }
}

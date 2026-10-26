/// 区分 XML 解析、必填属性、文档结构及目录标识冲突。
/// 底层错误与外部 ID 可能包含输入内容，不应直接写入生产日志。
#[derive(Debug)]
pub(crate) enum ImportError {
    Xml(quick_xml::Error),
    MissingAttribute(&'static str),
    EmptyAttribute(&'static str),
    /// 文档根节点不是 connections。
    InvalidRoot,
    /// 输入中没有根节点。
    MissingRoot,
    /// 已知节点层级非法、根节点重复或节点未闭合。
    InvalidStructure,
    /// 同一份导入文件中存在重复目录 ID。
    DuplicateFolderId(String),
}

impl From<quick_xml::Error> for ImportError {
    /// 保留 XML 错误原因，供调用方分类处理。
    fn from(err: quick_xml::Error) -> Self {
        Self::Xml(err)
    }
}

/// 第 7 讲的错误类型骨架；后续按解析失败类别补充，不携带凭据原文。
pub(crate) enum JaasError {}

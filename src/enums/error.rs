use std::error::Error;
use std::fmt;

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
    /// 认证属性只提供了一部分，无法完成校验。
    IncompleteAuthentication,
    /// 认证组合或 JAAS 配置无效。
    Authentication(AuthenticationError),
}

impl From<quick_xml::Error> for ImportError {
    /// 保留 XML 错误原因，供调用方分类处理。
    fn from(err: quick_xml::Error) -> Self {
        Self::Xml(err)
    }
}

impl fmt::Display for ImportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl Error for ImportError {}

/// 第 7 讲的错误类型骨架；后续按解析失败类别补充，不携带凭据原文。
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum JaasError {
    /// 应该遇到开引号，但没有	demo，或者空输入
    ExpectedQuote,
    /// 已经读到开引号，但直到末尾都没有闭引号	'demo
    UnterminatedQuote,
    /// 读到了反斜杠，但后面没有字符，转义没写完	'demo\
    IncompleteEscape,
    /// 反斜杠后面有字符，但不在我们支持的转义范围内	'demo\n'
    UnsupportedEscape,
    /// 当前位置应有未加引号的单词，但遇到了分隔符或输入结束。
    ExpectedWord,
    /// 模块名不在当前支持范围内；目前仅支持完整的 ScramLoginModule 类名。
    UnsupportedModule,
    /// 控制标记不在当前支持范围内；目前仅支持 required。
    UnsupportedControlFlag,
    /// 属性后面不是 '='
    ExpectedEquals,
    /// 同一个凭据字段重复出现，不能静默覆盖。
    DuplicateOption,
    /// 配置中缺少 username 字段。
    MissingUsername,
    /// 配置中缺少 password 字段。
    MissingPassword,
    /// 输入结束时仍未读到配置的结束分号。
    MissingSemicolon,
    /// 相邻键值项之间缺少必要的空白分隔。
    ExpectedWhitespace,
    /// 结束分号之后仍有非空白内容；当前不支持多个模块。
    TrailingContent,
}

impl fmt::Display for JaasError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl Error for JaasError {}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum AuthenticationError {
    /// 安全协议不在当前支持范围内。
    UnsupportedSecurityProtocol,
    /// SASL 机制不在当前支持范围内。
    UnsupportedSaslMechanism,
    /// JAAS 文本不符合当前支持的语法或模块要求。
    Jaas(JaasError),
}

impl fmt::Display for AuthenticationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl Error for AuthenticationError {}

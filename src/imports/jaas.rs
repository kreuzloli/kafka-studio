//! 第 7 讲：JAAS 带引号值解析练习，当前尚未实现。

use crate::enums::JaasError;

/// 带引号值读取函数的教学骨架，目前仅返回空字符串，不代表解析成功。
///
/// 后续约定：从开引号开始消费字符，停在闭引号之后；错误不包含输入片段。
fn read_quoted_value(
    chars: &mut std::iter::Peekable<std::str::Chars<'_>>,
) -> Result<String, JaasError> {
    // TODO: 下一次课程实现引号与受控转义解析，并补齐测试。
    Ok(String::new())
}

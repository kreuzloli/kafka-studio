use crate::enums::JaasError;

pub(super) const MODULE_NAME: &str = "org.apache.kafka.common.security.scram.ScramLoginModule";
pub(super) const MODULE_FLAG: &str = "required";

/// 后续约定：从开引号开始消费字符，停在闭引号之后；错误不包含输入片段。
fn read_quoted_value(
    chars: &mut std::iter::Peekable<std::str::Chars<'_>>,
) -> Result<String, JaasError> {
    let mut res = String::new();
    let begin = match chars.next() {
        Some(c) if c == '\'' || c == '"' => c,
        _ => return Err(JaasError::ExpectedQuote),
    };
    while let Some(current) = chars.next() {
        if current == begin {
            return Ok(res);
        }
        if current != '\\' {
            res.push(current);
            continue;
        }
        let next = chars.next().ok_or(JaasError::IncompleteEscape)?;
        if matches!(next, '\\' | '\'' | '"') {
            res.push(next);
        } else {
            return Err(JaasError::UnsupportedEscape);
        }
    }
    return Err(JaasError::UnterminatedQuote);
}

/// 跳过语法元素之间的空白，保留第一个非空白字符供后续解析。
fn skip_whitespace(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) {
    loop {
        match chars.peek() {
            Some(current) => {
                if current.is_whitespace() {
                    chars.next();
                    continue;
                }
                break;
            }
            None => break,
        }
    }
}

/// 读取未加引号的语法单词，保留分隔符供调用方处理。
fn read_word(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) -> Result<String, JaasError> {
    let mut res = String::new();
    loop {
        match chars.peek().copied() {
            Some(current) => {
                if current.is_whitespace() || matches!(current, '=' | ';' | '\'' | '"') {
                    break;
                }
                chars.next();
                res.push(current);
            }
            None => break,
        }
    }
    if !res.is_empty() {
        Ok(res)
    } else {
        Err(JaasError::ExpectedWord)
    }
}

/// 校验单模块 SCRAM 配置的开头，保留控制标记之后的分隔符。
fn parse_scram_header(
    chars: &mut std::iter::Peekable<std::str::Chars<'_>>,
) -> Result<(), JaasError> {
    // 读取模块
    skip_whitespace(chars);
    let module = read_word(chars)?;
    if module != MODULE_NAME {
        return Err(JaasError::UnsupportedModule);
    }
    // 读取标记
    skip_whitespace(chars);
    let flag = read_word(chars)?;
    if flag != MODULE_FLAG {
        return Err(JaasError::UnsupportedControlFlag);
    }
    Ok(())
}

/// 读取一个带引号值的键值项，停在值的闭引号之后。
fn read_option(
    chars: &mut std::iter::Peekable<std::str::Chars<'_>>,
) -> Result<(String, String), JaasError> {
    skip_whitespace(chars);
    let field = read_word(chars)?;
    skip_whitespace(chars);
    match chars.next() {
        Some(current) => {
            if current != '=' {
                return Err(JaasError::ExpectedEquals);
            }
            skip_whitespace(chars);
        }
        _ => return Err(JaasError::ExpectedEquals),
    }
    let value = read_quoted_value(chars)?;
    Ok((field, value))
}

#[cfg(test)]
mod tests {
    use crate::{
        enums::JaasError::{
            self, ExpectedQuote, ExpectedWord, IncompleteEscape, UnsupportedEscape,
            UnterminatedQuote,
        },
        imports::jaas::{
            parse_scram_header, read_option, read_quoted_value, read_word, skip_whitespace,
        },
    };

    #[test]
    fn reads_option_and_preserves_remaining_input() {
        let cases = [
            ("username='demo'", "username", "demo", ""),
            (
                "  username = 'demo reader' rest",
                "username",
                "demo reader",
                " rest",
            ),
            ("password=\"a=b;c\";", "password", "a=b;c", ";"),
        ];

        for (input, key, value, remaining) in cases {
            let mut chars = input.chars().peekable();

            assert_eq!(
                read_option(&mut chars),
                Ok((key.to_owned(), value.to_owned()))
            );
            // 值内的分隔符属于值，闭引号后的内容留给上层解析。
            assert_eq!(chars.collect::<String>(), remaining);
        }
    }

    /// 区分缺少等号与缺少带引号值，保留底层解析错误的类别。
    #[test]
    fn rejects_invalid_option_syntax() {
        let cases = [
            ("username 'demo'", JaasError::ExpectedEquals),
            ("username=", ExpectedQuote),
            ("username=demo", ExpectedQuote),
        ];

        for (input, expected) in cases {
            let mut chars = input.chars().peekable();

            assert_eq!(read_option(&mut chars), Err(expected));
        }
    }

    #[test]
    fn reads_scram_header_and_preserves_remaining_input() {
        let cases = [
            (
                "org.apache.kafka.common.security.scram.ScramLoginModule required username='demo'",
                " username='demo'",
            ),
            (
                " \torg.apache.kafka.common.security.scram.ScramLoginModule\n\trequired username='demo'",
                " username='demo'",
            ),
        ];

        for (input, remaining) in cases {
            let mut chars = input.chars().peekable();

            assert_eq!(parse_scram_header(&mut chars), Ok(()));
            // 开头校验不能消费后续字段或字段前的分隔符。
            assert_eq!(chars.collect::<String>(), remaining);
        }
    }

    /// 区分不支持的认证配置与缺少必要单词的输入。
    #[test]
    fn rejects_unsupported_or_incomplete_scram_header() {
        let cases = [
            ("OtherModule required", JaasError::UnsupportedModule),
            (
                "org.apache.kafka.common.security.scram.ScramLoginModule optional",
                JaasError::UnsupportedControlFlag,
            ),
            (
                "org.apache.kafka.common.security.scram.ScramLoginModule",
                ExpectedWord,
            ),
            ("", ExpectedWord),
        ];

        for (input, expected) in cases {
            let mut chars = input.chars().peekable();

            assert_eq!(parse_scram_header(&mut chars), Err(expected));
        }
    }

    #[test]
    fn leaves_remaining_input() {
        let mut chars = "'abc' rest".chars().peekable();

        assert_eq!(read_quoted_value(&mut chars), Ok("abc".to_owned()));
        assert_eq!(chars.next(), Some(' '));
        assert_eq!(chars.collect::<String>(), "rest");

        let cases: [(&str, Result<String, JaasError>); 11] = [
            (r#"'demo reader'"#, Ok("demo reader".to_string())),
            (r#""demo=123;""#, Ok("demo=123;".to_string())),
            (r#""""#, Ok(String::new())),
            (r#"'it\'s demo'"#, Ok("it's demo".to_string())),
            (r#""say \"hi\"""#, Ok("say \"hi\"".to_string())),
            (r#"'a\\b'"#, Ok("a\\b".to_string())),
            ("demo", Err(ExpectedQuote)),
            (r#"'unfinished"#, Err(UnterminatedQuote)),
            (r#"'demo\"#, Err(IncompleteEscape)),
            (r#"'demo\n'"#, Err(UnsupportedEscape)),
            ("", Err(ExpectedQuote)),
        ];
        for (input, expected) in cases {
            let res = read_quoted_value(&mut input.chars().peekable());
            assert_eq!(res, expected);
        }
    }

    #[test]
    fn test_skip_whitespace() {
        let cases = [
            ("  \t\nrequired", Some('r')),
            ("username", Some('u')),
            ("   ", None),
            ("", None),
            (r"\t", Some('\\')),
        ];
        for (input, expected) in cases {
            let mut chars = input.chars().peekable();
            skip_whitespace(&mut chars);
            assert_eq!(chars.next(), expected);
        }
    }
    #[test]
    fn test_read_word() {
        let cases = [
            (
                "ScramLoginModule required",
                Ok("ScramLoginModule".to_string()),
                Some(' '),
            ),
            ("username='demo'", Ok("username".to_string()), Some('=')),
            ("required;", Ok("required".to_string()), Some(';')),
            ("password", Ok("password".to_string()), None),
            ("=value", Err(ExpectedWord), Some('=')),
            (" username", Err(ExpectedWord), Some(' ')),
            ("", Err(ExpectedWord), None),
        ];
        for (input, expected, next) in cases {
            let mut chars = input.chars().peekable();
            let word = read_word(&mut chars);
            assert_eq!(word, expected);
            assert_eq!(next, chars.peek().copied());
        }
    }
}

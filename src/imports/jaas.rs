use std::{char, f64::consts::E};

use crate::{enums::JaasError, models::ScramCredentials};

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
) -> Result<(String, String), JaasError> {
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
    Ok((module, flag))
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

fn parse_scram_jaas(input: &str) -> Result<ScramCredentials, JaasError> {
    let mut username = None::<String>;
    let mut password = None::<String>;
    let mut chars = input.chars().peekable();
    parse_scram_header(&mut chars)?;
    loop {
        let has_whitespace = chars.peek().is_some_and(|c| c.is_whitespace());
        skip_whitespace(&mut chars);
        match chars.peek().copied() {
            Some(';') => {
                chars.next();
                break;
            }
            Some(_) => {}
            None => return Err(JaasError::MissingSemicolon),
        }
        if !has_whitespace {
            return Err(JaasError::ExpectedWhitespace);
        }
        let (field, value) = read_option(&mut chars)?;
        match field.to_lowercase().as_str() {
            "username" => {
                if username.is_some() {
                    return Err(JaasError::DuplicateOption);
                }
                username = Some(value);
            }
            "password" => {
                if password.is_some() {
                    return Err(JaasError::DuplicateOption);
                }
                password = Some(value);
            }
            _ => {}
        }
    }
    skip_whitespace(&mut chars);
    if chars.peek().is_some() {
        return Err(JaasError::TrailingContent);
    }
    let username = username.ok_or(JaasError::MissingUsername)?;
    let password = password.ok_or(JaasError::MissingPassword)?;
    Ok(ScramCredentials::new(username, password))
}

#[cfg(test)]
mod tests {
    use quick_xml::{Reader, events::Event, reader};

    use crate::{
        enums::JaasError::{
            self, ExpectedQuote, ExpectedWord, IncompleteEscape, UnsupportedEscape,
            UnterminatedQuote,
        },
        imports::jaas::{
            MODULE_FLAG, MODULE_NAME, parse_scram_header, parse_scram_jaas, read_option,
            read_quoted_value, read_word, skip_whitespace,
        },
        models::ScramCredentials,
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
            assert!(parse_scram_header(&mut chars).is_ok());
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

    #[test]
    fn parses_scram_credentials() -> Result<(), JaasError> {
        let options = [
            ("username='demo' password='Demo Pass';", "Demo Pass"),
            ("PASSWORD='Demo Pass' UserName='demo';", "Demo Pass"),
            (
                "username='demo' extra='a=b;c' password='Demo Pass';",
                "Demo Pass",
            ),
            ("username='demo' password='p'  ; \t\n", "p"),
            ("username='demo' password='p' extra='x';", "p"),
            ("username='demo' password=' p ';", " p "),
        ];

        for (option, expected_password) in options {
            let input = format!("{MODULE_NAME} {MODULE_FLAG} {option}");
            let credentials = parse_scram_jaas(&input)?;

            assert_eq!(credentials.username, "demo");
            assert_eq!(credentials.password, expected_password);
        }

        Ok(())
    }

    #[test]
    fn rejects_invalid_scram_configuration() {
        let cases = [
            (";", JaasError::MissingUsername),
            (
                "username='demo' password='p' extra='unfinished",
                JaasError::UnterminatedQuote,
            ),
            (
                "username='a' UserName='b' password='p';",
                JaasError::DuplicateOption,
            ),
            (
                "username='a' password='p' PASSWORD='q';",
                JaasError::DuplicateOption,
            ),
            ("password='p';", JaasError::MissingUsername),
            ("username='a';", JaasError::MissingPassword),
            ("username='a' password='p'", JaasError::MissingSemicolon),
            ("username='a'password='p';", JaasError::ExpectedWhitespace),
            (
                "username='a' password='p'; extra='x'",
                JaasError::TrailingContent,
            ),
            (
                "username='a' password='p' extra=;",
                JaasError::ExpectedQuote,
            ),
        ];
        for (option, expected) in cases {
            let input = format!("{MODULE_NAME} {MODULE_FLAG} {option}");
            match parse_scram_jaas(&input) {
                Err(actual) => assert_eq!(actual, expected),
                Ok(_) => panic!("Expected invalid configuration to be rejected"),
            }
        }
    }

    #[test]
    fn decodes_xml_entities_once() -> Result<(), Box<dyn std::error::Error>> {
        let mut res = None::<ScramCredentials>;
        let xml = r#"
        <connection jaas_config="org.apache.kafka.common.security.scram.ScramLoginModule required username=&quot;demo&quot; password=&quot;a&amp;b&amp;amp;c&quot;;" />
        "#;
        let mut reader = Reader::from_str(xml.trim());
        let event = reader.read_event()?;
        match &event {
            Event::Empty(e) => {
                for attr in e.attributes() {
                    let attr = attr.map_err(quick_xml::Error::from)?;
                    if attr.key.as_ref() == "jaas_config" {
                        let jaas = attr
                            .normalized_value(quick_xml::XmlVersion::Implicit1_0)?
                            .into_owned();
                        res = Some(parse_scram_jaas(&jaas)?);
                    }
                }
            }
            _ => {}
        }
        let credentials = res.expect("Expected jaas_config credentials");
        assert_eq!(credentials.username, "demo");
        assert_eq!(credentials.password, "a&b&amp;c");
        Ok(())
    }
}

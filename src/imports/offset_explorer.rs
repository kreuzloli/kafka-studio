//! Offset Explorer XML 的离线结构解析与内部目录 ID 转换。

use std::collections::{HashMap, HashSet};

use quick_xml::{
    Reader, XmlVersion,
    events::{BytesStart, Event},
};
use uuid::Uuid;

use crate::{
    enums::ImportError,
    imports::authentication,
    models::{
        Connection, ConnectionFolder, ConnectionFolderId, ConnectionImport, ImportedConnection,
        ImportedFolder, ScramCredentials,
    },
};

/// 检查必填属性；只判断空白，不修改原始有效文本。
fn required_attribute(
    value: Option<String>,
    attribute: &'static str,
) -> Result<String, ImportError> {
    let value = value.ok_or(ImportError::MissingAttribute(attribute))?;
    if value.trim().is_empty() {
        return Err(ImportError::EmptyAttribute(attribute));
    }
    Ok(value)
}

/// 读取 connection 标签的基本属性，并检查名称和地址是否缺失或为空。
pub(crate) fn parse_connection(
    element: &BytesStart<'_>,
    version: XmlVersion,
) -> Result<ImportedConnection, ImportError> {
    let mut conn = ImportedConnection::new();
    let mut name = None::<String>;
    let mut bootstrap_servers = None::<String>;
    let mut security_protocol = None::<String>;
    let mut sasl_mechanism = None::<String>;
    let mut jaas_config = None::<String>;
    for attr in element.attributes() {
        let attr = attr.map_err(quick_xml::Error::from)?;
        let key = attr.key.as_ref();
        // 只解码本阶段使用的属性，忽略无关字段的值。
        if !matches!(
            key,
            "name"
                | "bootstrap_servers"
                | "groupId"
                | "group"
                | "broker_security_type"
                | "sasl_mechanism"
                | "jaas_config"
        ) {
            continue;
        }
        let value = attr.normalized_value(version)?.into_owned();
        match key {
            "name" => name = Some(value),
            "bootstrap_servers" => bootstrap_servers = Some(value),
            "groupId" => conn.folder_external_id = Some(value),
            "group" => conn.folder_name_hint = Some(value),
            "broker_security_type" => security_protocol = Some(value),
            "sasl_mechanism" => sasl_mechanism = Some(value),
            "jaas_config" => jaas_config = Some(value),
            _ => {}
        }
    }
    conn.name = required_attribute(name, "name")?;
    conn.bootstrap_servers = required_attribute(bootstrap_servers, "bootstrap_servers")?;
    match (security_protocol, sasl_mechanism, jaas_config) {
        (None, None, None) => {}
        (Some(protocol), Some(mechanism), Some(jaas)) => {
            let credentials = authentication::parse_authentication(&protocol, &mechanism, &jaas)
                .map_err(ImportError::Authentication)?;
            conn.credentials = Some(credentials);
        }
        _ => return Err(ImportError::IncompleteAuthentication),
    }
    Ok(conn)
}

/// 读取目录属性，父目录由遍历 XML 的调用方提供。
fn parse_folder(
    element: &BytesStart<'_>,
    version: XmlVersion,
    parent_external_id: Option<String>,
) -> Result<ImportedFolder, ImportError> {
    let mut folder = ImportedFolder::new();
    let mut external_id = None::<String>;
    let mut name = None::<String>;
    for attr in element.attributes() {
        let attr = attr.map_err(quick_xml::Error::from)?;
        let key = attr.key.as_ref();
        if !matches!(key, "id" | "name") {
            continue;
        }
        let value = attr.normalized_value(version)?.into_owned();
        match key {
            "id" => external_id = Some(value),
            "name" => name = Some(value),
            _ => {}
        }
    }
    folder.external_id = required_attribute(external_id, "id")?;
    folder.name = required_attribute(name, "name")?;
    folder.parent_external_id = parent_external_id;
    Ok(folder)
}

/// 离线收集连接与目录，校验根节点、已知节点层级及目录 ID 唯一性。
///
/// 未知子树整体忽略；连接的目录引用留到转换时处理，不读取认证或证书资源。
fn parse_import(xml: &str) -> Result<ConnectionImport, ImportError> {
    let mut reader = Reader::from_str(xml);
    let mut version = XmlVersion::Implicit1_0;
    // 目录栈只描述业务父子关系，元素栈另外负责 XML 层级校验。
    let mut folder_stack = Vec::<String>::new();
    let mut res = ConnectionImport::new();
    let mut root_seen = false;
    let mut element_stack = Vec::<String>::new();
    loop {
        let event = reader.read_event()?;
        // 先借用事件验证结构，再消费同一事件生成业务记录。
        match &event {
            Event::Start(e) | Event::Empty(e) => {
                let e_name = e.name();
                let name = e_name.as_ref();
                match element_stack.last().map(String::as_str) {
                    None => {
                        if root_seen {
                            return Err(ImportError::InvalidStructure);
                        }
                        if name != "connections" {
                            return Err(ImportError::InvalidRoot);
                        }
                        root_seen = true;
                    }
                    Some(parent) => {
                        // 整体跳过未知子树，避免把扩展内容中的同名标签当成连接或目录。
                        if !matches!(name, "connections" | "connection" | "groups" | "group") {
                            if matches!(&event, Event::Start(_)) {
                                reader.read_to_end(e.name())?;
                            }
                            continue;
                        }
                        let allowed = matches!(
                            (parent, name),
                            ("connections", "connection" | "groups")
                                | ("groups" | "group", "group")
                        );
                        if !allowed {
                            return Err(ImportError::InvalidStructure);
                        }
                    }
                }
                // 自闭合标签没有对应的 End 事件，因此不入栈。
                if matches!(&event, Event::Start(_)) {
                    element_stack.push(name.to_owned());
                }
            }
            Event::End(e) => {
                let e_name = e.name();
                if element_stack.last().map(String::as_str) != Some(e_name.as_ref()) {
                    return Err(ImportError::InvalidStructure);
                }
                element_stack.pop();
            }
            _ => {}
        }
        match event {
            Event::Start(e) if e.name().as_ref() == "group" => {
                let folder = parse_folder(&e, version, folder_stack.last().cloned())?;
                folder_stack.push(folder.external_id.clone());
                res.folders.push(folder);
            }
            Event::Empty(e) if e.name().as_ref() == "group" => {
                let folder = parse_folder(&e, version, folder_stack.last().cloned())?;
                res.folders.push(folder);
            }
            Event::End(e) if e.name().as_ref() == "group" => {
                folder_stack.pop();
            }
            Event::Start(e) | Event::Empty(e) if e.name().as_ref() == "connection" => {
                let conn = parse_connection(&e, version)?;
                res.connections.push(conn);
            }
            Event::Decl(declaration) => {
                version = declaration.xml_version()?;
            }
            Event::Eof => break,
            _ => {}
        }
    }
    if !root_seen {
        return Err(ImportError::MissingRoot);
    }
    if !element_stack.is_empty() {
        return Err(ImportError::InvalidStructure);
    }
    // 重复外部 ID 会让后续映射覆盖旧值，必须在转换前拒绝。
    let mut folder_ids = HashSet::<&str>::new();
    for folder in &res.folders {
        if !folder_ids.insert(folder.external_id.as_str()) {
            return Err(ImportError::DuplicateFolderId(folder.external_id.clone()));
        }
    }
    // 只记录汇总数量，不输出 XML、属性值或底层错误原文。
    eprintln!(
        "[kafka-studio] event=import_parsed connections={} folders={}",
        res.connections.len(),
        res.folders.len()
    );
    Ok(res)
}

/// 消费已通过解析校验的记录，为本次导入生成独立的内部目录 ID。
///
/// 保留空目录与原有顺序；连接引用的目录不存在时，保留连接并放入未分组区域。
fn convert_import(imported: ConnectionImport) -> (Vec<ConnectionFolder>, Vec<Connection>) {
    let mut folder_ids = HashMap::<String, ConnectionFolderId>::new();
    let mut folders = Vec::<ConnectionFolder>::new();
    let mut connections = Vec::<Connection>::new();
    // 先完整建立映射，父子引用转换不依赖目录在输入中的先后顺序。
    for folder in imported.folders.iter() {
        let connection_folder_id = ConnectionFolderId(Uuid::new_v4().to_string());
        folder_ids.insert(folder.external_id.clone(), connection_folder_id);
    }
    // 按值迭代会移动字段，避免复制目录名称等自有字符串。
    for folder in imported.folders {
        // 上一轮已为每个目录插入 ID，因此这里可以直接索引。
        let id = folder_ids[&folder.external_id].clone();
        let parent_id = folder
            .parent_external_id
            .as_ref()
            .and_then(|id| folder_ids.get(id))
            .cloned();
        folders.push(ConnectionFolder {
            id,
            name: folder.name,
            parent_id,
        });
    }

    for (connection_index, conn) in imported.connections.into_iter().enumerate() {
        let folder_id = conn
            .folder_external_id
            .as_ref()
            .and_then(|id| folder_ids.get(id))
            .cloned();
        // 区分原本未分组与引用丢失；日志只使用本次导入中的序号。
        if conn.folder_external_id.is_some() && folder_id.is_none() {
            eprintln!(
                "[kafka-studio] event=import_folder_reference_missing connection_index={connection_index}"
            );
        }
        connections.push(Connection {
            name: conn.name,
            bootstrap_servers: conn.bootstrap_servers,
            folder_id,
            credentials: conn.credentials,
        });
    }

    eprintln!(
        "[kafka-studio] event=import_converted connections={} folders={}",
        connections.len(),
        folders.len()
    );
    (folders, connections)
}

#[cfg(test)]
mod tests {

    use std::collections::HashMap;

    use crate::{
        enums::ImportError,
        imports::offset_explorer::{convert_import, parse_folder, parse_import},
        models::{Connection, ConnectionFolder, ConnectionFolderId, ImportedFolder},
    };

    use super::parse_connection;
    use quick_xml::{XmlVersion, events::Event, reader::Reader};

    #[test]
    fn imports_optional_authentication() -> Result<(), ImportError> {
        let basic = parse_import(
            r#"<connections><connection name="Demo" bootstrap_servers="localhost:9093" /></connections>"#,
        )?;
        assert!(basic.connections[0].credentials.is_none());

        let imported = parse_import(
            r#"<connections><connection name="Demo" bootstrap_servers="localhost:9093"
                broker_security_type="SASL_SSL" sasl_mechanism="SCRAM-SHA-512"
                jaas_config="org.apache.kafka.common.security.scram.ScramLoginModule required username=&quot;demo&quot; password=&quot;a&amp;b&amp;amp;c&quot;;" />
            </connections>"#,
        )?;
        let credentials = imported.connections[0]
            .credentials
            .as_ref()
            .expect("Expected imported credentials");
        assert_eq!(credentials.username, "demo");
        // 正式导入入口也必须只解码一次 XML 实体。
        assert_eq!(credentials.password.expose(), "a&b&amp;c");
        Ok(())
    }

    #[test]
    fn rejects_each_missing_authentication_attribute() {
        let attributes = [
            r#"broker_security_type="SASL_SSL""#,
            r#"sasl_mechanism="SCRAM-SHA-512""#,
            r#"jaas_config="org.apache.kafka.common.security.scram.ScramLoginModule required username='demo' password='p';""#,
        ];
        for missing in 0..attributes.len() {
            let remaining = attributes
                .iter()
                .enumerate()
                .filter_map(|(index, attribute)| (index != missing).then_some(*attribute))
                .collect::<Vec<_>>()
                .join(" ");
            let xml = format!(
                r#"<connections><connection name="Demo" bootstrap_servers="localhost:9093" {remaining} /></connections>"#
            );
            assert!(matches!(
                parse_import(&xml),
                Err(ImportError::IncompleteAuthentication)
            ));
        }
    }

    #[test]
    fn preserves_import_authentication_errors() {
        use crate::enums::{AuthenticationError, JaasError};

        let cases = [
            (
                "SSL",
                "username='demo' password='p';",
                AuthenticationError::UnsupportedSecurityProtocol,
            ),
            (
                "SASL_SSL",
                "username='demo';",
                AuthenticationError::Jaas(JaasError::MissingPassword),
            ),
        ];
        for (protocol, options, expected) in cases {
            let xml = format!(
                r#"<connections><connection name="Demo" bootstrap_servers="localhost:9093"
                    broker_security_type="{protocol}" sasl_mechanism="SCRAM-SHA-512"
                    jaas_config="org.apache.kafka.common.security.scram.ScramLoginModule required {options}" />
                </connections>"#
            );
            match parse_import(&xml) {
                Err(ImportError::Authentication(actual)) => assert_eq!(actual, expected),
                _ => panic!("Expected an authentication error from XML import"),
            }
        }
    }

    #[test]
    fn reads_basic_xml_events() -> Result<(), quick_xml::Error> {
        let xml = include_str!("../../tests/fixtures/offset_explorer_basic.xml");
        let mut reader = Reader::from_str(xml);
        let mut tags = Vec::<(&str, String)>::new();
        loop {
            match reader.read_event()? {
                Event::Start(e) => {
                    tags.push(("start", e.name().as_ref().to_owned()));
                }
                Event::Empty(e) => {
                    tags.push(("empty", e.name().as_ref().to_owned()));
                }
                Event::End(e) => {
                    tags.push(("end", e.name().as_ref().to_owned()));
                }
                Event::Eof => break,
                _ => {}
            }
        }
        assert_eq!(
            tags,
            vec![
                ("start", "connections".to_owned()),
                ("empty", "connection".to_owned()),
                ("start", "groups".to_owned()),
                ("start", "group".to_owned()),
                ("empty", "group".to_owned()),
                ("end", "group".to_owned()),
                ("empty", "group".to_owned()),
                ("end", "groups".to_owned()),
                ("end", "connections".to_owned()),
            ]
        );
        Ok(())
    }

    #[test]
    fn reads_connection_name_attribute() -> Result<(), quick_xml::Error> {
        let mut reader = Reader::from_str(r#"<connection name="Local &amp; Demo" />"#);
        let mut name = None::<String>;
        if let Event::Empty(e) = reader.read_event()? {
            assert_eq!(e.name().as_ref(), "connection");
            for attr in e.attributes() {
                let attr = attr?;
                if attr.key.as_ref() == "name" {
                    // Implicit1_0 表示这段没有 XML 声明的样例按 XML 1.0 处理。
                    // into_owned() 将返回的 Cow<str> 转为自有的 String；需要时复制，已经拥有字符串时直接取出。
                    name = Some(attr.normalized_value(XmlVersion::Implicit1_0)?.into_owned());
                }
            }
        }
        assert_eq!(name.as_deref(), Some("Local & Demo"));
        Ok(())
    }

    #[test]
    fn reads_connection_attributes() -> Result<(), ImportError> {
        let mut reader = Reader::from_str(
            r#"<connection
                name="Local &amp; Demo"
                bootstrap_servers="localhost:9092"
                group="Local"
                groupId="2"
            />"#,
        );
        let Event::Empty(element) = reader.read_event()? else {
            panic!("Expected an empty connection element");
        };
        assert_eq!(element.name().as_ref(), "connection");
        let connection = parse_connection(&element, XmlVersion::Implicit1_0)?;
        assert_eq!(connection.name, "Local & Demo");
        assert_eq!(connection.bootstrap_servers, "localhost:9092");
        assert_eq!(connection.folder_external_id.as_deref(), Some("2"));
        assert_eq!(connection.folder_name_hint.as_deref(), Some("Local"));
        Ok(())
    }

    #[test]
    fn rejects_missing_or_empty_connection_name() -> Result<(), ImportError> {
        let cases = [
            (r#"<connection bootstrap_servers="localhost:9092" />"#, true),
            (
                r#"<connection name="" bootstrap_servers="localhost:9092" />"#,
                false,
            ),
            (
                r#"<connection name="   " bootstrap_servers="localhost:9092" />"#,
                false,
            ),
        ];

        for (xml, missing) in cases {
            let mut reader = Reader::from_str(xml);
            let Event::Empty(e) = reader.read_event()? else {
                panic!("Expected an empty connection element");
            };
            let conn = parse_connection(&e, XmlVersion::Implicit1_0);
            if missing {
                assert!(matches!(conn, Err(ImportError::MissingAttribute("name"))));
            } else {
                assert!(matches!(conn, Err(ImportError::EmptyAttribute("name"))));
            }
        }
        Ok(())
    }

    #[test]
    fn reads_folder_attributes() -> Result<(), ImportError> {
        let mut reader = Reader::from_str(r#"<group id="2" name="Local &amp; Dev" />"#);
        let Event::Empty(e) = reader.read_event()? else {
            panic!("Expected an empty folder element");
        };
        let folder = parse_folder(&e, XmlVersion::Implicit1_0, Some("1".to_owned()))?;
        assert_eq!(folder.external_id, "2");
        assert_eq!(folder.name, "Local & Dev");
        assert_eq!(folder.parent_external_id.as_deref(), Some("1"));
        Ok(())
    }

    #[test]
    fn reads_nested_folders() -> Result<(), ImportError> {
        let xml = r#"
                <groups>
                    <group id="1" name="Development">
                        <group id="2" name="Local" />
                    </group>
                    <group id="3" name="Empty" />
                </groups>
            "#;
        let mut reader = Reader::from_str(xml);
        let mut folder_stack = Vec::<String>::new();
        let mut folders = Vec::<ImportedFolder>::new();
        loop {
            match reader.read_event()? {
                Event::Start(e) if e.name().as_ref() == "group" => {
                    let folder =
                        parse_folder(&e, XmlVersion::Implicit1_0, folder_stack.last().cloned())?;
                    folder_stack.push(folder.external_id.clone());
                    folders.push(folder);
                }
                Event::Empty(e) if e.name().as_ref() == "group" => {
                    let folder =
                        parse_folder(&e, XmlVersion::Implicit1_0, folder_stack.last().cloned())?;
                    folders.push(folder);
                }
                Event::End(e) if e.name().as_ref() == "group" => {
                    folder_stack.pop();
                }
                Event::Eof => break,
                _ => {}
            }
        }
        assert_eq!(folders.len(), 3);

        assert_eq!(folders[0].external_id, "1");
        assert_eq!(folders[0].parent_external_id, None);

        assert_eq!(folders[1].external_id, "2");
        assert_eq!(folders[1].parent_external_id.as_deref(), Some("1"));

        assert_eq!(folders[2].external_id, "3");
        assert_eq!(folders[2].parent_external_id, None);

        assert!(folder_stack.is_empty());
        Ok(())
    }

    #[test]
    fn reads_basic_import() -> Result<(), ImportError> {
        let xml = include_str!("../../tests/fixtures/offset_explorer_basic.xml");
        let res = parse_import(xml)?;
        assert_eq!(res.connections.len(), 1);
        assert_eq!(res.folders.len(), 3);

        assert_eq!(res.connections[0].name, "Local Demo");
        assert_eq!(res.connections[0].folder_external_id.as_deref(), Some("2"));

        assert_eq!(res.folders[1].external_id, "2");
        assert_eq!(res.folders[1].parent_external_id.as_deref(), Some("1"));
        assert_eq!(res.folders[2].name, "Empty");
        assert_eq!(res.folders[2].parent_external_id, None);
        Ok(())
    }

    #[test]
    fn rejects_unknown_xml_version() {
        let xml = r#"<?xml version="9.9"?><connections />"#;
        assert!(matches!(parse_import(xml), Err(ImportError::Xml(_))));
    }

    /// 区分缺少根节点、根节点名称错误和合法空文件。
    #[test]
    fn checks_import_root() -> Result<(), ImportError> {
        assert!(matches!(parse_import(""), Err(ImportError::MissingRoot)));
        assert!(matches!(
            parse_import("<settings />"),
            Err(ImportError::InvalidRoot)
        ));
        let result = parse_import("<connections />")?;
        assert!(result.connections.is_empty());
        assert!(result.folders.is_empty());
        Ok(())
    }

    /// 拒绝未闭合、多根及已知节点位置错误的结构。
    #[test]
    fn rejects_invalid_import_structure() {
        let cases = [
            // 根元素没有闭合。
            "<connections>",
            // 一份文件出现两个根元素。
            "<connections /><connections />",
            // 连接位于根元素外。
            r#"<connections />
               <connection name="Demo" bootstrap_servers="localhost:9092" />"#,
            // 目录应位于 groups 或另一个 group 内。
            r#"<connections>
                   <group id="1" name="Development" />
               </connections>"#,
        ];

        for xml in cases {
            assert!(
                parse_import(xml).is_err(),
                "Expected invalid structure to be rejected: {xml}"
            );
        }
    }

    /// 防止同一外部目录 ID 在转换时覆盖已有映射。
    #[test]
    fn rejects_duplicate_folder_ids() {
        let xml = r#"
            <connections>
                <groups>
                    <group id="1" name="Development" />
                    <group id="1" name="Production" />
                </groups>
            </connections>
        "#;

        assert!(matches!(
            parse_import(xml),
            Err(ImportError::DuplicateFolderId(id)) if id == "1"
        ));
    }

    /// 无关属性不参与解码，避免其未知实体阻断基础字段导入。
    #[test]
    fn ignores_unused_attribute_values() -> Result<(), ImportError> {
        let xml = r#"
            <connections>
                <connection
                    name="Demo"
                    bootstrap_servers="localhost:9092"
                    groupId="1"
                    unused="&custom;" />
                <groups>
                    <group id="1" name="Development" unused="&custom;" />
                </groups>
            </connections>
        "#;

        let result = parse_import(xml)?;

        assert_eq!(result.connections.len(), 1);
        assert_eq!(result.connections[0].name, "Demo");
        assert_eq!(
            result.connections[0].folder_external_id.as_deref(),
            Some("1")
        );
        assert_eq!(result.folders.len(), 1);
        assert_eq!(result.folders[0].external_id, "1");

        Ok(())
    }

    /// 未知子树中的同名业务标签不能混入导入结果。
    #[test]
    fn ignores_unknown_elements() -> Result<(), ImportError> {
        let xml = r#"
            <connections>
                <metadata>
                    <connection name="Example" />
                    <group id="unused" name="Example folder" />
                </metadata>
                <extra />
                <connection name="Demo" bootstrap_servers="localhost:9092" />
            </connections>
        "#;

        let result = parse_import(xml)?;

        assert_eq!(result.connections.len(), 1);
        assert_eq!(result.connections[0].name, "Demo");
        assert!(result.folders.is_empty());

        Ok(())
    }

    /// 验证内部 ID 同时替换父目录引用和连接归属，并保留空目录。
    #[test]
    fn maps_external_folder_ids() -> Result<(), ImportError> {
        let xml = include_str!("../../tests/fixtures/offset_explorer_basic.xml");
        let imported = parse_import(xml)?;
        let (folders, connections) = convert_import(imported);

        assert_eq!(folders.len(), 3);
        assert_eq!(folders[0].name, "Development");
        assert_eq!(folders[0].parent_id, None);

        assert_eq!(folders[1].name, "Local");
        assert_eq!(folders[1].parent_id.as_ref(), Some(&folders[0].id));

        assert_eq!(folders[2].name, "Empty");
        assert_eq!(folders[2].parent_id, None);

        assert_eq!(connections.len(), 1);
        assert_eq!(connections[0].name, "Local Demo");
        assert_eq!(connections[0].bootstrap_servers, "localhost:9092");
        assert_eq!(connections[0].folder_id.as_ref(), Some(&folders[1].id));

        Ok(())
    }

    /// 重复导入同一文件也应生成独立 ID，避免不同批次目录混用。
    #[test]
    fn assigns_new_folder_ids_for_each_import() -> Result<(), ImportError> {
        let xml = include_str!("../../tests/fixtures/offset_explorer_basic.xml");

        let (first_folders, first_connections) = convert_import(parse_import(xml)?);
        let (second_folders, second_connections) = convert_import(parse_import(xml)?);

        for first in &first_folders {
            assert!(second_folders.iter().all(|second| first.id != second.id));
        }

        assert_eq!(
            first_connections[0].folder_id.as_ref(),
            Some(&first_folders[1].id)
        );
        assert_eq!(
            second_connections[0].folder_id.as_ref(),
            Some(&second_folders[1].id)
        );

        Ok(())
    }

    /// 目录引用缺失时只取消分组，不丢弃连接。
    #[test]
    fn keeps_connection_when_folder_is_missing() -> Result<(), ImportError> {
        let xml = r#"
            <connections>
                <connection
                    name="Demo"
                    bootstrap_servers="localhost:9092"
                    groupId="missing" />
            </connections>
        "#;

        let (folders, connections) = convert_import(parse_import(xml)?);

        assert!(folders.is_empty());
        assert_eq!(connections.len(), 1);
        assert_eq!(connections[0].name, "Demo");
        assert_eq!(connections[0].bootstrap_servers, "localhost:9092");
        assert_eq!(connections[0].folder_id, None);

        Ok(())
    }

    /// 用四连接、八目录样例检查完整层级、空目录和未分组连接。
    #[test]
    fn converts_nested_import() -> Result<(), ImportError> {
        let xml = include_str!("../../tests/fixtures/offset_explorer_nested.xml");
        let (folders, connections) = convert_import(parse_import(xml)?);
        assert_eq!(folders.len(), 8);
        assert_eq!(connections.len(), 4);
        // XML 中的目录按先父后子的顺序收集。
        let expected_folders: [(&str, Option<usize>); 8] = [
            ("Clusters", None),
            ("Development", Some(0)),
            ("Local", Some(1)),
            ("Shared", Some(1)),
            ("Production", Some(0)),
            ("Region A", Some(4)),
            ("Empty Region", Some(4)),
            ("Archive", None),
        ];
        for (folder, (name, parent_index)) in folders.iter().zip(expected_folders) {
            assert_eq!(folder.name, name);
            assert_eq!(
                folder.parent_id.as_ref(),
                parent_index.map(|index| &folders[index].id)
            );
        }
        let expected_connections: [(&str, Option<usize>); 4] = [
            ("Local", Some(2)),
            ("Dev", Some(3)),
            ("Production", Some(5)),
            ("Standalone", None),
        ];

        for (connection, (name, folder_index)) in connections.iter().zip(expected_connections) {
            assert_eq!(connection.name, name);
            assert_eq!(
                connection.folder_id.as_ref(),
                folder_index.map(|index| &folders[index].id)
            );
        }

        Ok(())
    }

    #[test]
    fn converts_credentials_without_exposing_them_in_debug() -> Result<(), ImportError> {
        let xml = r#"
        <connections>
            <connection
                name="Demo"
                bootstrap_servers="localhost:9093"
                broker_security_type="SASL_SSL"
                sasl_mechanism="SCRAM-SHA-512"
                jaas_config="org.apache.kafka.common.security.scram.ScramLoginModule required username='DEMO_USER_ONLY' password='DEMO_PASSWORD_ONLY';" />
        </connections>
        "#;
        let imported = parse_import(xml)?;
        let (_, connections) = convert_import(imported);

        assert_eq!(connections.len(), 1);
        let connection = &connections[0];
        let credentials = connection
            .credentials
            .as_ref()
            .expect("Expected credentials after conversion");

        assert_eq!(credentials.username, "DEMO_USER_ONLY");
        assert_eq!(credentials.password.expose(), "DEMO_PASSWORD_ONLY");

        for output in [format!("{connection:?}"), format!("{connection:#?}")] {
            assert!(!output.contains("DEMO_USER_ONLY"));
            assert!(!output.contains("DEMO_PASSWORD_ONLY"));
            assert!(output.contains("[REDACTED]"));
        }
        Ok(())
    }
}

use std::collections::HashSet;

use crate::{
    enums::ConnectionTreeRow,
    models::{Connection, ConnectionFolder, ConnectionFolderId},
};

/// 将有效的目录树展开为显示行；同一层先展示目录，再展示连接。
pub(super) fn build_connection_rows(
    folders: &[ConnectionFolder],
    connections: &[Connection],
    collapsed_folders: &HashSet<ConnectionFolderId>,
) -> Vec<ConnectionTreeRow> {
    let mut rows = Vec::<ConnectionTreeRow>::new();
    append_rows(None, 0, folders, connections, collapsed_folders, &mut rows);
    rows
}

/// 追加指定父目录下的内容，进入子目录时增加缩进层级。
fn append_rows(
    parent_id: Option<&ConnectionFolderId>,
    depth: usize,
    folders: &[ConnectionFolder],
    connections: &[Connection],
    collapsed_folders: &HashSet<ConnectionFolderId>,
    rows: &mut Vec<ConnectionTreeRow>,
) {
    for (folder_index, folder) in folders.iter().enumerate() {
        if folder.parent_id.as_ref() != parent_id {
            continue;
        }

        rows.push(ConnectionTreeRow::Folder {
            folder_index,
            depth,
        });
        if !collapsed_folders.contains(&folder.id) {
            append_rows(
                Some(&folder.id),
                depth + 1,
                folders,
                connections,
                collapsed_folders,
                rows,
            );
        }
    }

    for (connection_index, connection) in connections.iter().enumerate() {
        if connection.folder_id.as_ref() == parent_id {
            rows.push(ConnectionTreeRow::Connection {
                connection_index,
                depth,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::build_connection_rows;
    use crate::{
        enums::ConnectionTreeRow::{Connection as ConnectionRow, Folder},
        models::{Connection, ConnectionFolder, ConnectionFolderId},
    };

    #[test]
    fn builds_nested_rows_and_preserves_connection_indices() {
        let development_id = ConnectionFolderId("development".to_owned());
        let local_id = ConnectionFolderId("local".to_owned());

        let folders = vec![
            ConnectionFolder {
                id: development_id.clone(),
                name: "Development".to_owned(),
                parent_id: None,
            },
            ConnectionFolder {
                id: local_id.clone(),
                name: "Local".to_owned(),
                parent_id: Some(development_id.clone()),
            },
            ConnectionFolder {
                id: ConnectionFolderId("empty".to_owned()),
                name: "Empty".to_owned(),
                parent_id: None,
            },
        ];

        let connections = vec![
            Connection {
                name: "Root connection".to_owned(),
                bootstrap_servers: "localhost:9092".to_owned(),
                folder_id: None,
                credentials: None,
            },
            Connection {
                name: "Nested connection".to_owned(),
                bootstrap_servers: "localhost:9093".to_owned(),
                folder_id: Some(local_id),
                credentials: None,
            },
            Connection {
                name: "Development connection".to_owned(),
                bootstrap_servers: "localhost:9094".to_owned(),
                folder_id: Some(development_id),
                credentials: None,
            },
        ];
        let mut collapsed_folders = HashSet::<ConnectionFolderId>::new();
        let rows = build_connection_rows(&folders, &connections, &collapsed_folders);

        assert_eq!(
            rows,
            vec![
                Folder {
                    folder_index: 0,
                    depth: 0,
                },
                Folder {
                    folder_index: 1,
                    depth: 1,
                },
                ConnectionRow {
                    connection_index: 1,
                    depth: 2,
                },
                ConnectionRow {
                    connection_index: 2,
                    depth: 1,
                },
                Folder {
                    folder_index: 2,
                    depth: 0,
                },
                ConnectionRow {
                    connection_index: 0,
                    depth: 0,
                },
            ]
        );
        collapsed_folders.insert(folders[0].id.clone());
        let rows = build_connection_rows(&folders, &connections, &collapsed_folders);
        assert_eq!(
            rows,
            vec![
                Folder {
                    folder_index: 0,
                    depth: 0,
                },
                Folder {
                    folder_index: 2,
                    depth: 0,
                },
                ConnectionRow {
                    connection_index: 0,
                    depth: 0,
                },
            ]
        );
        // 子目录先折叠，再收起并展开父目录，子目录仍保持折叠。
        collapsed_folders.clear();
        collapsed_folders.insert(folders[1].id.clone());
        collapsed_folders.insert(folders[0].id.clone());
        collapsed_folders.remove(&folders[0].id);

        let rows = build_connection_rows(&folders, &connections, &collapsed_folders);
        assert_eq!(
            rows,
            vec![
                Folder {
                    folder_index: 0,
                    depth: 0,
                },
                Folder {
                    folder_index: 1,
                    depth: 1
                },
                ConnectionRow {
                    connection_index: 2,
                    depth: 1,
                },
                Folder {
                    folder_index: 2,
                    depth: 0,
                },
                ConnectionRow {
                    connection_index: 0,
                    depth: 0,
                },
            ]
        )
    }
}

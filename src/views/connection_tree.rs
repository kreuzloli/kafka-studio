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
        // 折叠仅跳过子行，目录本身仍显示，原始连接索引也保持不变。
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

/// 收集指定目录及所有后代的 ID，不受侧栏折叠状态影响。
pub(super) fn collect_folder_subtree_ids(
    folders: &[ConnectionFolder],
    root_id: &ConnectionFolderId,
) -> HashSet<ConnectionFolderId> {
    let mut ids = HashSet::new();
    let mut pending = vec![root_id];

    while let Some(folder_id) = pending.pop() {
        // 已处理过的目录无需再次展开。
        if !ids.insert(folder_id.clone()) {
            continue;
        }

        for folder in folders {
            if folder.parent_id.as_ref() == Some(folder_id) {
                pending.push(&folder.id);
            }
        }
    }

    ids
}

#[cfg(test)]
mod tests {
    use super::{build_connection_rows, collect_folder_subtree_ids};
    use crate::{
        enums::ConnectionTreeRow::{Connection as ConnectionRow, Folder},
        models::{Connection, ConnectionFolder, ConnectionFolderId},
    };
    use std::collections::HashSet;

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
                id: uuid::Uuid::new_v4().to_string(),
                name: "Root connection".to_owned(),
                bootstrap_servers: "localhost:9092".to_owned(),
                folder_id: None,
                credentials: None,
                truststore_location: None,
                ca_location: None,
            },
            Connection {
                id: uuid::Uuid::new_v4().to_string(),
                name: "Nested connection".to_owned(),
                bootstrap_servers: "localhost:9093".to_owned(),
                folder_id: Some(local_id),
                credentials: None,
                truststore_location: None,
                ca_location: None,
            },
            Connection {
                id: uuid::Uuid::new_v4().to_string(),
                name: "Development connection".to_owned(),
                bootstrap_servers: "localhost:9094".to_owned(),
                folder_id: Some(development_id),
                credentials: None,
                truststore_location: None,
                ca_location: None,
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

    #[test]
    fn collects_subtree_without_unrelated_folders() {
        let root_id = ConnectionFolderId("root".to_owned());
        let child_id = ConnectionFolderId("child".to_owned());
        let grandchild_id = ConnectionFolderId("grandchild".to_owned());
        let unrelated_id = ConnectionFolderId("unrelated".to_owned());

        // 故意将后代放在父目录之前，验证收集结果不依赖存储顺序。
        let folders = vec![
            ConnectionFolder {
                id: grandchild_id.clone(),
                name: "Grandchild".to_owned(),
                parent_id: Some(child_id.clone()),
            },
            ConnectionFolder {
                id: child_id.clone(),
                name: "Child".to_owned(),
                parent_id: Some(root_id.clone()),
            },
            ConnectionFolder {
                id: root_id.clone(),
                name: "Root".to_owned(),
                parent_id: None,
            },
            ConnectionFolder {
                id: unrelated_id,
                name: "Unrelated".to_owned(),
                parent_id: None,
            },
        ];

        let actual = collect_folder_subtree_ids(&folders, &root_id);
        let expected = HashSet::from([root_id, child_id, grandchild_id]);

        assert_eq!(actual, expected);
    }
}

//! 只读客户端的 Demo 外壳：连接选择、目录折叠与键盘导航。

use std::{collections::HashSet, fs, time::Duration};

use async_channel::{Receiver, Sender};
use gpui::{
    Div, Entity, FocusHandle, KeyDownEvent, PathPromptOptions, PromptLevel, ScrollHandle, Window,
    div, prelude::*, px, rgb, rgba,
};
use gpui_component::{
    button::{Button, ButtonVariants},
    input::{Escape, InputEvent, InputState, TextInput},
    popup_menu::PopupMenuExt,
};

use crate::{
    components::selectable_text,
    enums::ConnectionTreeRow,
    imports,
    models::{Connection, ConnectionFolder, ConnectionFolderId, ImportPreview},
    views::{connection_tree, connection_workspace},
};
// 点击 New folder  → 触发 NewFolder → 视图收到动作 → 调用 add_folder
gpui::actions!(kafka_studio, [NewFolder, NewConnection]);

/// 根视图保存交互状态，子布局根据这些状态生成界面。
#[derive(Debug)]
pub struct KafkaStudio {
    /// 选中的示例配置索引；Some 不代表已经连接 Kafka。
    selected_connection: Option<usize>,
    /// 目录可见性独立于选择状态，折叠不清除选择。
    expanded: bool,
    /// 跨渲染保留焦点身份，避免每次构建界面重新申请焦点。
    focus_handle: FocusHandle,
    /// 当前会话的连接配置，供侧栏和工作区共同读取。
    connections: Vec<Connection>,
    /// 选中连接的名称草稿；选择时加载，仅在点击保存并通过校验后写回内存配置。
    name_input: Entity<InputState>,
    /// 最近一次连接配置保存的表单校验提示。
    form_error: Option<&'static str>,
    /// Broker 地址草稿，选择或重置时加载，通过校验后与名称一起写回内存配置。
    bootstrap_input: Entity<InputState>,
    /// 保留侧栏滚动状态，新增连接时用于滚到底部。
    sidebar_scroll: ScrollHandle,
    /// 当前会话的连接目录，连接通过 folder_id 引用其中的目录。
    folders: Vec<ConnectionFolder>,
    /// 已折叠的目录；折叠只影响可见行，不清除选择和编辑草稿。
    collapsed_folders: HashSet<ConnectionFolderId>,
    /// 文件选择或读取进行中，避免重复启动任务。
    ca_selection_pending: bool,
    /// 待用户确认的导入数据，与当前连接列表分开保存。
    import_preview: Option<ImportPreview>,
    /// XML 文件选择与解析进行中，避免重复启动导入。
    import_pending: bool,
    /// 导入操作的提示，与连接编辑错误分开。
    import_error: Option<&'static str>,
    /// 新增菜单对应的父目录；None 表示根层级。
    add_menu_parent_id: Option<ConnectionFolderId>,
    /// 正在重命名的目录；None 表示未进入编辑状态。
    renaming_folder_id: Option<ConnectionFolderId>,
    /// 文件夹名称草稿，与连接名称输入框独立。
    folder_name_input: Entity<InputState>,
    /// 重命名校验提示。
    folder_name_error: Option<&'static str>,
    fake_query_pending: bool,
    fake_query_result: Option<&'static str>,
    /// 每次启动查询或切换连接时递增，使旧请求的结果失效。
    fake_query_request_id: u64,
    /// 在视图存活期间持续驱动后台任务，避免每次查询创建运行时。
    runtime: tokio::runtime::Runtime,
}

impl KafkaStudio {
    /// 创建初始状态并取得窗口焦点，使根视图可以接收键盘事件。
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();
        focus_handle.focus(window);

        let name_input =
            cx.new(|cx| InputState::new(window, cx).placeholder("Enter a connection name"));

        let bootstrap_input =
            cx.new(|cx| InputState::new(window, cx).placeholder("localhost:9092"));
        eprintln!("[kafka-studio] event=view_initialized selected=None expanded=true");

        let folder_name_input =
            cx.new(|cx| InputState::new(window, cx).placeholder("Enter a folder name"));

        // 输入框处理 Enter 后，通过事件通知根视图保存草稿。
        cx.subscribe_in(
            &folder_name_input,
            window,
            |this, _input, event: &InputEvent, window: &mut Window, cx| {
                if let InputEvent::PressEnter { .. } = event {
                    this.save_folder_name(window, cx);
                }
            },
        )
        .detach();

        let development_id = ConnectionFolderId("development".to_owned());
        Self {
            selected_connection: None,
            expanded: true,
            focus_handle,
            connections: vec![
                Connection {
                    id: uuid::Uuid::new_v4().to_string(),
                    name: String::from("Local Demo"),
                    bootstrap_servers: String::from("localhost:9092"),
                    folder_id: Some(development_id.clone()),
                    credentials: None,
                    truststore_location: None,
                    ca_location: None,
                },
                Connection {
                    id: uuid::Uuid::new_v4().to_string(),
                    name: String::from("Test Demo"),
                    bootstrap_servers: String::from("test.example:9092"),
                    folder_id: None,
                    credentials: None,
                    truststore_location: None,
                    ca_location: None,
                },
            ],
            name_input,
            form_error: None,
            bootstrap_input,
            sidebar_scroll: ScrollHandle::new(),
            folders: vec![ConnectionFolder {
                id: development_id,
                name: "Development".to_owned(),
                parent_id: None,
            }],
            collapsed_folders: HashSet::<ConnectionFolderId>::new(),
            ca_selection_pending: false,
            import_preview: None,
            import_pending: false,
            import_error: None,
            add_menu_parent_id: None,
            renaming_folder_id: None,
            folder_name_input,
            folder_name_error: None,
            fake_query_pending: false,
            fake_query_result: None,
            fake_query_request_id: 0,
            runtime: tokio::runtime::Builder::new_multi_thread()
                .enable_time()
                .build()
                .expect("Cannot initialize background runtime"),
        }
    }

    /// 处理 Esc 清除、上下循环选择、左右折叠展开；隐藏的连接不参与上下导航。
    fn handle_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // 输入框获得焦点时，方向键交给文本编辑，根视图不同时执行导航。
        if !self.focus_handle.is_focused(window) {
            return;
        }
        // 只记录导航状态的实际变化，不记录无关按键或输入文字。
        let previous_state = (self.selected_connection, self.expanded);
        match event.keystroke.key.as_str() {
            "escape" => {
                self.select_connection(None, window, cx);
            }
            "down" | "up" if self.expanded => {
                let visible_connections: Vec<usize> = connection_tree::build_connection_rows(
                    &self.folders,
                    &self.connections,
                    &self.collapsed_folders,
                )
                .into_iter()
                .filter_map(|e| match e {
                    ConnectionTreeRow::Connection {
                        connection_index, ..
                    } => Some(connection_index),
                    ConnectionTreeRow::Folder { .. } => None,
                })
                .collect();
                if visible_connections.is_empty() {
                    return;
                }
                let current_position = visible_connections
                    .iter()
                    .position(|index| Some(*index) == self.selected_connection);
                let next_position = match current_position {
                    None => 0,
                    Some(position) if event.keystroke.key == "down" => {
                        (position + 1) % visible_connections.len()
                    }
                    Some(0) => visible_connections.len() - 1,
                    Some(position) => position - 1,
                };
                let connection_index = visible_connections[next_position];
                self.select_connection(Some(connection_index), window, cx);
            }
            "left" | "right" => {
                self.expanded = !self.expanded;
                cx.notify();
            }
            _ => {}
        }
        if previous_state != (self.selected_connection, self.expanded) {
            eprintln!(
                "[kafka-studio] event=keyboard_navigation key={} selected={:?} expanded={}",
                event.keystroke.key, self.selected_connection, self.expanded
            );
        }
    }
    /// 更新选择并加载名称与地址草稿；重新选择或重置会覆盖两个字段的未保存内容。
    fn select_connection(
        &mut self,
        selected: Option<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.form_error = None;
        self.selected_connection = selected;
        self.fake_query_pending = false;
        self.fake_query_result = None;
        self.fake_query_request_id += 1;
        let (name, bootstrap_servers) = match selected {
            Some(index) => {
                let connection: &Connection = &self.connections[index];
                (
                    connection.name.clone(),
                    connection.bootstrap_servers.clone(),
                )
            }
            None => (String::new(), String::new()),
        };
        self.name_input.update(cx, |input, cx| {
            input.set_value(name, window, cx);
        });
        self.bootstrap_input.update(cx, |input, cx| {
            input.set_value(bootstrap_servers, window, cx);
        });
        if let Some(index) = selected {
            let rows = connection_tree::build_connection_rows(
                &self.folders,
                &self.connections,
                &self.collapsed_folders,
            );
            let row_index = rows.iter().position(|row| {
                matches!(
                    row,
                    ConnectionTreeRow::Connection { connection_index, .. } if *connection_index == index
                )
            });
            if let Some(row_index) = row_index {
                self.sidebar_scroll.scroll_to_item(row_index);
            }
        }
        cx.notify();
    }
    /// 先校验名称与地址均非空，再一起写入内存配置，避免只保存部分字段。
    fn save_connection(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(index) = self.selected_connection else {
            self.form_error = Some("Select a connection first");
            cx.notify();
            return;
        };
        let value = self.name_input.read(cx).value(); // read(cx) 借用输入状态；我们只读取
        let value = value.trim();
        if value.is_empty() {
            self.form_error = Some("Connection name cannot be empty");
            cx.notify();
            return;
        }
        let bootstrap_value = self.bootstrap_input.read(cx).value();
        let bootstrap_servers = bootstrap_value.trim();
        if bootstrap_servers.is_empty() {
            self.form_error = Some("Bootstrap servers cannot be empty");
            cx.notify();
            return;
        }
        let bootstrap_servers = match Connection::normalize_bootstrap_servers(bootstrap_servers) {
            Ok(servers) => servers,
            Err(message) => {
                self.form_error = Some(message);
                cx.notify();
                return;
            }
        };

        let connection = &mut self.connections[index];
        connection.name = value.to_owned();
        connection.bootstrap_servers = bootstrap_servers;

        self.select_connection(Some(index), window, cx);
        eprintln!("[kafka-studio] event=connection_saved index={index}");
    }

    /// 新增目录并展开其父目录，保留当前连接选择和编辑草稿。
    fn add_folder(&mut self, parent_id: Option<ConnectionFolderId>, cx: &mut Context<Self>) {
        let mut number = self.folders.len() + 1;
        let mut name = format!("New folder {number}");
        while self.folders.iter().any(|folder| folder.name == name) {
            number += 1;
            name = format!("New folder {number}");
        }
        // 先借用父目录 ID 展开目录，再将所有权移入新目录。
        if let Some(id) = parent_id.as_ref() {
            self.collapsed_folders.remove(id);
        }
        self.folders.push(ConnectionFolder {
            id: ConnectionFolderId(uuid::Uuid::new_v4().to_string()),
            name,
            parent_id,
        });
        self.expanded = true;
        eprintln!("[kafka-studio] event=folder_added");
        cx.notify();
    }

    /// 添加当前会话的连接配置，生成未被占用的默认名称并选中新行。
    fn add_connection(
        &mut self,
        folder_id: Option<ConnectionFolderId>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let index = self.connections.len();
        let mut number = index + 1;
        let mut name = format!("New connection {number}");

        while self.connections.iter().any(|conn| conn.name == name) {
            number += 1;
            name = format!("New connection {number}");
        }
        self.connections.push(Connection {
            id: uuid::Uuid::new_v4().to_string(),
            name,
            bootstrap_servers: String::from("localhost:9092"),
            folder_id,
            credentials: None,
            truststore_location: None,
            ca_location: None,
        });

        self.expanded = true;
        self.select_connection(Some(index), window, cx);
        eprintln!("[kafka-studio] event=connection_added index={index}");
    }

    /// 删除指定连接；删除其他行时保留当前选择和编辑草稿。
    fn delete_connection(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.connections.remove(index);
        match self.selected_connection {
            Some(selected) => {
                if selected == index {
                    self.select_connection(None, window, cx);
                }
                if selected > index {
                    self.selected_connection = Some(selected - 1);
                }
            }
            _ => {}
        }
        eprintln!("[kafka-studio] event=connection_deleted index={index}");
        cx.notify();
    }

    /// 展示删除范围，用户确认后再删除目录及其连接。
    fn request_delete_folder(
        &mut self,
        folder_id: ConnectionFolderId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let folder_ids = connection_tree::collect_folder_subtree_ids(&self.folders, &folder_id);
        let connection_count = self
            .connections
            .iter()
            .filter(|conn| {
                conn.folder_id
                    .as_ref()
                    .is_some_and(|id| folder_ids.contains(id))
            })
            .count();
        let detail = format!(
            "This will delete {} folders (including this folder) and {} connections. \
                 This cannot be undone.",
            folder_ids.len(),
            connection_count,
        );
        let answer = window.prompt(
            PromptLevel::Warning,
            "Delete this folder and its contents?",
            Some(&detail),
            &["Cancel", "Delete"],
            cx,
        );
        // detach() 的意思是：放开任务句柄，让任务继续运行，不在这里等待它完成。
        // spawn_in() 会返回一个 Task 句柄。在 GPUI 中，直接丢弃这个句柄会取消任务；调用 .detach() 则允许任务继续执行。
        cx.spawn_in(window, async move |this, cx| {
            if let Ok(1) = answer.await {
                // 等待期间视图可能关闭；关闭后无需再执行界面操作。
                let _ = this.update_in(cx, |this, window, cx| {
                    this.delete_folder(&folder_id, window, cx);
                });
            }
        })
        .detach();
    }

    /// 删除目录及其后代；保留未被删除连接的选择和编辑草稿。
    fn delete_folder(
        &mut self,
        folder_id: &ConnectionFolderId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let folder_ids = connection_tree::collect_folder_subtree_ids(&self.folders, folder_id);
        // 批量删除会改变索引，先记录选中连接的稳定 ID。
        let selected_id = self
            .selected_connection
            .map(|index| self.connections[index].id.clone());
        let previous_connection_count = self.connections.len();
        let previous_folder_count = self.folders.len();
        // retain 的含义是“保留满足条件的元素”。
        self.connections.retain(|conn| {
            !conn
                .folder_id
                .as_ref()
                .is_some_and(|id| folder_ids.contains(id))
        });
        self.folders
            .retain(|folder| !folder_ids.contains(&folder.id));
        self.collapsed_folders.retain(|id| !folder_ids.contains(id));
        if let Some(selected_id) = selected_id {
            match self
                .connections
                .iter()
                .position(|conn| conn.id == selected_id)
            {
                Some(index) => self.selected_connection = Some(index),
                None => self.select_connection(None, window, cx),
            }
        }
        eprintln!(
            "[kafka-studio] event=folder_deleted folders={} connections={}",
            previous_folder_count - self.folders.len(),
            previous_connection_count - self.connections.len(),
        );
        cx.notify();
    }

    /// 选择本机 CA 文件，在后台检查可读性，再更新发起操作的连接。
    fn select_ca_file(&mut self, cx: &mut Context<Self>) {
        if self.ca_selection_pending {
            return;
        }
        let Some(index) = self.selected_connection else {
            self.form_error = Some("Select a connection first");
            cx.notify();
            return;
        };
        // 等待期间用户可能切换或删除连接，因此记录稳定 ID，不保留列表索引。
        let connection_id = self.connections[index].id.clone();
        self.ca_selection_pending = true;
        self.form_error = None;
        cx.notify();
        let selection = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Select PEM CA file".into()),
        });
        let executor = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let res = match selection.await {
                Ok(Ok(Some(paths))) => {
                    if let Some(path) = paths.into_iter().next() {
                        executor.spawn(async move {
                            // 只检查可读性，不解析 PEM，也不验证 TLS。
                            fs::read(&path).map(|_|Some(path)).map_err(|_| "Cannot read CA file")
                        }).await
                    } else {
                        Ok(None)
                    }
                }
                Ok(Ok(None)) => Ok(None),
                _=> Err("Cannot open file picker")
            };
            // 视图可能已关闭，此时无需再更新界面。
            let _ = this.update(cx, |this, cx| {
                this.ca_selection_pending = false;
                if let Some(conn) = this.connections.iter_mut().find(|conn| conn.id == connection_id) {
                    match res {
                        Ok(Some(path)) => {
                            conn.ca_location = Some(path);
                            eprintln!(
                                "[kafka-studio] event=ca_file_selected connection_id={connection_id}"
                            );
                        },
                        Ok(None) => {},
                        Err(msg) => {
                            eprintln!(
                                "[kafka-studio] event=ca_selection_failed connection_id={connection_id}"
                            );
                            // 错误只显示在发起操作的连接上。
                            if this.selected_connection.is_some_and(|index| {
                                this.connections[index].id == connection_id
                            }) {
                                this.form_error = Some(msg);
                            }
                        }
                    }
                }
                cx.notify();
            });
        }).detach();
    }

    /// 选择 XML 并在后台准备预览；成功前不修改当前连接列表。
    fn select_import_file(&mut self, cx: &mut Context<Self>) {
        if self.import_pending {
            return;
        }
        self.import_pending = true;
        self.import_error = None;
        cx.notify();
        let selection = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Select Offset Explorer XML".into()),
        });
        let executor = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let res = match selection.await {
                Ok(Ok(Some(paths))) => {
                    if let Some(path) = paths.into_iter().next() {
                        executor
                            .spawn(async move {
                                let xml = fs::read_to_string(&path)
                                    .map_err(|_| "Cannot read XML file")?;
                                let preview = imports::prepare_import(&xml)
                                    .map_err(|_| "Cannot parse connection XML")?;
                                Ok(Some(preview))
                            })
                            .await
                    } else {
                        Ok(None)
                    }
                }
                Ok(Ok(None)) => Ok(None),
                _ => Err("Cannot open file picker"),
            };
            let _ = this.update(cx, |this, cx| {
                this.import_pending = false;
                match res {
                    Ok(Some(preview)) => this.import_preview = Some(preview),
                    Ok(None) => {}
                    Err(msg) => this.import_error = Some(msg),
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// 将预览中的目录和连接追加到当前会话，确认后清空预览。
    fn confirm_import(&mut self, cx: &mut Context<Self>) {
        if self.import_pending {
            return;
        }
        let Some(preivew) = self.import_preview.take() else {
            return;
        };
        let folder_count = preivew.folders.len();
        let connection_count = preivew.connections.len();
        // extend() 消费这两个集合，把元素追加进去。导入转换已经生成新的内部 ID，不需要在这里重新生成。
        self.folders.extend(preivew.folders);
        self.connections.extend(preivew.connections);
        self.expanded = true;
        self.import_error = None;
        println!(
            "[kafka-studio] event=import_confirmed connections={connection_count} folders={folder_count}"
        );
        cx.notify();
    }

    /// 丢弃尚未确认的数据，不修改当前连接列表。
    fn cancel_import(&mut self, cx: &mut Context<Self>) {
        if self.import_pending {
            return;
        }
        self.import_preview = None;
        self.import_error = None;
        cx.notify();
    }

    /// 在菜单指定的父目录下新增文件夹。
    fn handle_new_folder(&mut self, _: &NewFolder, _window: &mut Window, cx: &mut Context<Self>) {
        let parent_id = self.add_menu_parent_id.clone();
        self.add_folder(parent_id, cx);
    }

    /// 在菜单指定的目录下新增连接，并展开该目录。
    fn handle_new_connection(
        &mut self,
        _: &NewConnection,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let folder_id = self.add_menu_parent_id.clone();
        if let Some(id) = folder_id.as_ref() {
            self.collapsed_folders.remove(id);
        }
        self.add_connection(folder_id, window, cx);
    }

    /// 将目录名称加载为草稿，进入编辑时不修改已保存名称。
    fn begin_rename_folder(
        &mut self,
        folder_id: ConnectionFolderId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(folder) = self.folders.iter().find(|folder| folder.id == folder_id) else {
            return;
        };
        let name = folder.name.clone();
        self.renaming_folder_id = Some(folder_id);
        self.folder_name_error = None;
        self.folder_name_input.update(cx, |input, cx| {
            input.set_value(name, window, cx);
            input.focus(window, cx);
        });
        cx.notify();
    }

    /// 校验名称后更新目录，目录 ID 和子项归属保持不变。
    fn save_folder_name(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(folder_id) = self.renaming_folder_id.as_ref() else {
            return;
        };
        let value = self.folder_name_input.read(cx).value();
        let name = value.trim();
        if name.is_empty() {
            self.folder_name_error = Some("Folder name cannot be empty");
            cx.notify();
            return;
        }
        if let Some(folder) = self
            .folders
            .iter_mut()
            .find(|folder| &folder.id == folder_id)
        {
            folder.name = name.to_owned();
            eprintln!("[kafka-studio] event=folder_renamed");
        }
        self.cancel_rename_folder(window, cx);
    }

    /// 退出名称编辑；草稿未写入目录，因此取消无需恢复旧名称。
    fn cancel_rename_folder(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.renaming_folder_id = None;
        self.folder_name_error = None;
        self.focus_handle.focus(window);
        cx.notify();
    }

    pub(super) fn start_fake_query(
        &mut self,
        delay: Duration,
        result: &'static str,
        cx: &mut Context<Self>,
    ) {
        self.fake_query_pending = true;
        self.fake_query_result = None;
        self.fake_query_request_id += 1;
        let request_id = self.fake_query_request_id;
        cx.notify();
        let (sender, receiver): (Sender<&str>, Receiver<&str>) = async_channel::bounded(1);
        self.runtime.spawn(async move {
            tokio::time::sleep(delay).await;
            let _ = sender.send(result).await;
        });
        cx.spawn(async move |this, cx| {
            if let Ok(result) = receiver.recv().await {
                let _ = this.update(cx, |studio, cx| {
                    if studio.fake_query_request_id != request_id {
                        return;
                    }
                    studio.fake_query_pending = false;
                    studio.fake_query_result = Some(result);
                    cx.notify();
                });
            }
        })
        .detach();
    }
}

impl Render for KafkaStudio {
    /// 根据当前状态构建布局；渲染可能频繁发生，因此这里不输出交互日志。
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let selected_connection = self
            .selected_connection
            .map(|index| &self.connections[index]);
        let run_fast_query = Button::new("run-fast-query")
            .cursor_pointer()
            .text()
            .label("Run Fast Query")
            .on_click(cx.listener(move |this, _event, _window, cx| {
                this.start_fake_query(Duration::from_secs(1), "Fast query completed", cx);
            }));
        let run_slow_query = Button::new("run-slow-query")
            .cursor_pointer()
            .text()
            .label("Run Slow Query")
            .on_click(cx.listener(move |this, _event, _window, cx| {
                this.start_fake_query(Duration::from_secs(3), "Slow query completed", cx);
            }));
        let cancel_fake_query = Button::new("cancel-fake-query")
            .cursor_pointer()
            .text()
            .label("Cancel Fake Query")
            .on_click(cx.listener(move |this, _event, _window, cx| {
                this.fake_query_request_id += 1;
                this.fake_query_pending = false;
                cx.notify();
            }));
        // 将按钮行单独构建，避免事件回调淹没主布局的容器层级。
        let connection_actions = div()
            .flex()
            .flex_row()
            .gap_1()
            .p_2()
            .child(
                div()
                    .id("reset-fields")
                    .p_2()
                    .cursor_pointer()
                    .child("Reset fields")
                    .on_click(cx.listener(|this, _event, window, cx| {
                        let selected = this.selected_connection;
                        this.select_connection(selected, window, cx);
                        eprintln!(
                            "[kafka-studio] event=connection_draft_reset selected={selected:?}"
                        );
                    })),
            )
            .child(
                div()
                    .id("save-connection")
                    .p_2()
                    .cursor_pointer()
                    .child("Save connection")
                    .on_click(cx.listener(|this, _event, window, cx| {
                        this.save_connection(window, cx);
                    })),
            )
            .child(
                div()
                    .id("select-ca-file")
                    .p_2()
                    .cursor_pointer()
                    .child(if self.ca_selection_pending {
                        "Selecting CA..."
                    } else {
                        "Select CA file"
                    })
                    .on_click(cx.listener(|this, _event, _window, cx| {
                        this.select_ca_file(cx);
                    })),
            )
            .child(
                div()
                    .id("select-import-file")
                    .p_2()
                    .cursor_pointer()
                    .child(if self.import_pending {
                        "Importing..."
                    } else {
                        "Import XML"
                    })
                    .on_click(cx.listener(|this, _event, _window, cx| {
                        this.select_import_file(cx);
                    })),
            );

        div()
            .track_focus(&self.focus_handle)
            // 传入方法而非调用方法，由 listener 在事件到来时取得可变实体。
            .on_key_down(cx.listener(Self::handle_key_down))
            .on_action(cx.listener(Self::handle_new_folder))
            .on_action(cx.listener(Self::handle_new_connection))
            .size_full()
            .flex()
            .flex_col()
            .bg(rgba(0x18181be6))
            .text_color(rgb(0xffffff))
            .child(top_menu())
            .child(
                div()
                    .flex()
                    .flex_1() // 分配剩余空间
                    .min_h_0() // 允许这个容器小于内部内容的自然高度，否则内容可能继续把它撑大。
                    .text_sm()
                    .child(
                        div()
                            .id("connection-sidebar")
                            .w(px(240.0))
                            .h_full()
                            .min_h_0()
                            .flex_shrink_0()
                            .border_r_1()
                            .border_color(rgb(0x475569))
                            .child(connection_list(
                                cx,
                                self.selected_connection,
                                self.expanded,
                                &self.connections,
                                &self.sidebar_scroll,
                                &self.folders,
                                &self.collapsed_folders,
                                self.renaming_folder_id.as_ref(),
                                &self.folder_name_input,
                                self.folder_name_error
                            )),
                    )
                    .child(
                        div()
                            .id("connection-panel-scroll")
                            .flex()
                            .flex_col()
                            .flex_1()
                            .h_full()
                            .min_h_0()
                            .min_w_0()
                            .overflow_y_scroll()
                            .child(connection_workspace::connection_workspace(
                                selected_connection,
                                run_fast_query,
                                run_slow_query,
                                cancel_fake_query,
                                &self.folders,
                                self.fake_query_pending,
                                self.fake_query_result,
                                window,
                                cx,
                            ))
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .flex_shrink_0()
                                    .gap_2()
                                    .p_6()
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap_1()
                                            .p_2()
                                            .child(div().w(px(180.0)).flex_shrink_0().child(
                                                selectable_text(
                                                    "name-label",
                                                    "Connection name:",
                                                    window,
                                                    cx,
                                                ),
                                            ))
                                            .child(TextInput::new(&self.name_input)),
                                    )
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap_1()
                                            .p_2()
                                            .child(div().w(px(180.0)).flex_shrink_0().child(
                                                selectable_text(
                                                    "bootstrap-label",
                                                    "Bootstrap servers:",
                                                    window,
                                                    cx,
                                                ),
                                            ))
                                            .child(TextInput::new(&self.bootstrap_input)),
                                    )
                                    .child(connection_actions)
                                    .when_some(self.import_preview.as_ref(), |this, preview| {
                                        let connection_rows =
                                            preview.connections.iter().map(|conn| {
                                                let truststore = conn
                                                    .truststore_location
                                                    .as_deref()
                                                    .unwrap_or("not provided");
                                                div()
                                                    .child(format!(
                                                        "Connection: {}",
                                                        conn.name
                                                    ))
                                                    .child(format!(
                                                        "Imported truststore: {truststore}"
                                                    ))
                                                    .child("Local CA: not selected")
                                                    .when_some(connection_workspace::certificate_hint(conn), |this, message| {this.child(div().child(message))})
                                            });
                                        this.child(
                                                div()
                                                    .flex()
                                                    .flex_col()
                                                    .gap_2()
                                                    .child(selectable_text(
                                                        "import-preview-summary",
                                                        format!(
                                                            "Import preview: {} connections, {} folders",
                                                            preview.connections.len(),
                                                            preview.folders.len(),
                                                        ),
                                                        window,
                                                        cx,
                                                    ))
                                                    .child(div().id("import-preview-scroll").max_h(px(180.0)).overflow_y_scroll().children(connection_rows))
                                                    .child(
                                                        div()
                                                            .flex()
                                                            .gap_2()
                                                            .child(
                                                                div()
                                                                    .id("confirm_import")
                                                                    .p_2()
                                                                    .cursor_pointer()
                                                                    .child("Confirm import")
                                                                    .on_click(cx.listener(|this, _event, _window, cx| {
                                                                        this.confirm_import(cx);
                                                                    }))
                                                            )
                                                            .child(
                                                                div()
                                                                    .id("cancel-import")
                                                                    .p_2()
                                                                    .cursor_pointer()
                                                                    .child("Cancel")
                                                                    .on_click(cx.listener(|this, _event, _window, cx| {
                                                                        this.cancel_import(cx);
                                                                    }))
                                                            )
                                                    )
                                            )
                                    })
                                    .when_some(self.import_error, |this, message| {
                                        this.child(selectable_text(
                                            "import-error",
                                            message,
                                            window,
                                            cx,
                                        ))
                                    })
                                    .when_some(self.form_error, |this, message| {
                                        this.child(selectable_text(
                                            "form-error",
                                            message,
                                            window,
                                            cx,
                                        ))
                                    }),
                            ),
                    ),
            )
    }
}

/// 构建静态顶栏，不持有交互状态。
pub(super) fn top_menu() -> Div {
    div()
        .flex()
        .items_center()
        .px_4()
        .h(px(48.0))
        .flex_shrink_0()
        .text_lg()
        .child("Rust Kafka Studio")
}

/// 根据选择与展开状态构建侧栏，通过 listener 将鼠标操作写回根视图。
pub(super) fn connection_list(
    cx: &mut Context<KafkaStudio>,
    selected_connection: Option<usize>,
    expanded: bool,
    connections: &[Connection],
    scroll_handle: &ScrollHandle,
    folders: &[ConnectionFolder],
    collapsed_folders: &HashSet<ConnectionFolderId>,
    renaming_folder_id: Option<&ConnectionFolderId>,
    folder_name_input: &Entity<InputState>,
    folder_name_error: Option<&'static str>,
) -> Div {
    let studio = cx.entity().downgrade();
    let root_add_button =
        Button::new("add-root")
            .text()
            .label("+")
            .popup_menu(move |menu, _window, cx| {
                let Ok(focus_handle) = studio.update(cx, |this, _cx| {
                    this.add_menu_parent_id = None;
                    this.focus_handle.clone()
                }) else {
                    return menu;
                };
                menu.action_context(focus_handle)
                    .menu("New folder", Box::new(NewFolder))
                    .menu("New connection", Box::new(NewConnection))
            });
    let mut list = div()
        .flex()
        .flex_col()
        .w_full()
        .h_full()
        .min_h_0()
        .p_6()
        .child(format!("Connections ({})", connections.len()))
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .flex_shrink_0()
                .p_2()
                .child(
                    div()
                        .id("folder")
                        .flex_1()
                        .min_w_0()
                        .cursor_pointer()
                        .child(if expanded {
                            "▼ All connections"
                        } else {
                            "▶ All connections"
                        })
                        .on_click(cx.listener(|this, _evnet, window, cx| {
                            this.focus_handle.focus(window);
                            this.expanded = !this.expanded;
                            cx.notify();
                        })),
                )
                .child(root_add_button),
        );
    if expanded {
        let mut connection_rows = div()
            .id("connection-rows-scroll")
            .flex()
            .flex_col()
            .items_start() // 不再把每一行横向拉伸成固定的列表宽度，让长行按内容扩展。
            .flex_1()
            .min_h_0()
            .pl(px(16.0))
            .overflow_scroll()
            .track_scroll(scroll_handle);
        let rows = connection_tree::build_connection_rows(folders, connections, collapsed_folders);
        if rows.is_empty() {
            connection_rows = connection_rows.child(
                div()
                    .p_2()
                    .text_color(rgb(0x94a3b8))
                    .child("No connections. Click + beside All connections."),
            );
        }
        for row in rows {
            match row {
                ConnectionTreeRow::Folder {
                    folder_index,
                    depth,
                } => {
                    let folder = &folders[folder_index];
                    let folder_id = folder.id.clone();
                    let delete_folder_id = folder.id.clone();
                    let collapsed = collapsed_folders.contains(&folder_id);
                    let arrow = if collapsed { "▶" } else { "▼" };

                    let rename_folder_id = folder.id.clone();
                    let is_renaming = renaming_folder_id == Some(&folder.id);

                    let folder_label = div()
                        .flex_grow()
                        .flex_shrink_0()
                        .when(is_renaming, |this| {
                            this.child(
                                div()
                                    .w(px(160.0))
                                    .flex()
                                    .flex_col()
                                    .gap_1()
                                    .on_action(cx.listener(|this, _: &Escape, window, cx| {
                                        this.cancel_rename_folder(window, cx);
                                    }))
                                    .child(TextInput::new(folder_name_input))
                                    .when_some(folder_name_error, |this, message| {
                                        this.child(
                                            div()
                                                .text_sm()
                                                .text_color(rgb(0xef4444))
                                                .child(message),
                                        )
                                    }),
                            )
                        })
                        .when(!is_renaming, |this| {
                            this.child(
                                div()
                                    .id(("folder-label", folder_index))
                                    .whitespace_nowrap()
                                    .cursor_pointer()
                                    .child(folder.name.clone())
                                    .on_click(cx.listener(
                                        move |this, event: &gpui::ClickEvent, window, cx| {
                                            if event.click_count() == 2 {
                                                this.begin_rename_folder(
                                                    rename_folder_id.clone(),
                                                    window,
                                                    cx,
                                                );
                                            }
                                        },
                                    )),
                            )
                        });

                    let studio = cx.entity().downgrade();
                    let add_folder_id = folder.id.clone();
                    let folder_add_button = Button::new(("add-folder-child", folder_index))
                        .text()
                        .label("+")
                        .popup_menu(move |menu, _window, cx| {
                            let Ok(focus_handle) = studio.update(cx, |this, _cx| {
                                this.add_menu_parent_id = Some(add_folder_id.clone());
                                this.focus_handle.clone()
                            }) else {
                                return menu;
                            };
                            menu.action_context(focus_handle)
                                .menu("New folder", Box::new(NewFolder))
                                .menu("New connection", Box::new(NewConnection))
                        });
                    connection_rows = connection_rows.child(
                        div()
                            .id(("connection-folder", folder_index))
                            .flex()
                            .items_center()
                            .gap_2()
                            .min_w_full()
                            .flex_shrink_0()
                            .p_2()
                            .pl(px(8.0 + depth as f32 * 16.0))
                            .text_color(rgb(0x94a3b8))
                            .cursor_pointer()
                            .child(
                                div()
                                    .id(("folder-toggle", folder_index))
                                    .flex_shrink_0()
                                    .cursor_pointer()
                                    .child(arrow)
                                    .on_click(cx.listener(move |this, _event, window, cx| {
                                        this.focus_handle.focus(window);
                                        if !this.collapsed_folders.remove(&folder_id) {
                                            this.collapsed_folders.insert(folder_id.clone());
                                        }
                                        cx.notify();
                                    })),
                            )
                            .child(folder_label)
                            .child(folder_add_button)
                            .child(
                                div()
                                    .id(("delete-folder", folder_index))
                                    .flex_shrink_0()
                                    .px_2()
                                    .cursor_pointer()
                                    .child("-")
                                    .on_click(cx.listener(move |this, _event, window, cx| {
                                        // 删除按钮不触发整行的折叠操作。
                                        cx.stop_propagation();
                                        this.request_delete_folder(
                                            delete_folder_id.clone(),
                                            window,
                                            cx,
                                        );
                                    })),
                            ),
                    );
                }
                ConnectionTreeRow::Connection {
                    connection_index,
                    depth,
                } => {
                    let connection: &Connection = &connections[connection_index];
                    connection_rows = connection_rows.child(
                        div()
                            .id(("connection", connection_index))
                            .flex()
                            .items_center()
                            .gap_2()
                            .min_w_full() // 短行仍至少铺满列表，保留整行选中背景。
                            .flex_shrink_0()
                            .bg(if selected_connection == Some(connection_index) {
                                rgba(0x334155e6)
                            } else {
                                rgba(0x18181be6)
                            })
                            .p_2()
                            .pl(px(8.0 + depth as f32 * 16.0))
                            .cursor_pointer()
                            .child(
                                div()
                                    .flex_grow() // 名称较短时占用剩余空间。
                                    .flex_shrink_0() // 名称较长时不被强制压缩。
                                    .whitespace_nowrap() // 文字始终保持单行。
                                    .child(connection.name.clone()),
                            )
                            .child(
                                div()
                                    .id(("delete-connection", connection_index))
                                    .flex_shrink_0()
                                    .px_2()
                                    .cursor_pointer()
                                    .child("-")
                                    .on_click(cx.listener(move |this, _event, window, cx| {
                                        cx.stop_propagation();
                                        this.delete_connection(connection_index, window, cx);
                                    }))
                            )
                            .on_click(cx.listener(move |this, _event, window, cx| {
                                this.focus_handle.focus(window);
                                this.select_connection(Some(connection_index), window, cx);
                                eprintln!("[kafka-studio] event=connection_selected source=mouse index={connection_index} connected=false");
                            })),
                    )
                }
            }
        }
        list = list.child(connection_rows);
    }
    // 清除入口位于折叠区域之外，目录隐藏时仍然可用。
    list = list.child(
        div()
            .id("clear-selection")
            .flex_shrink_0()
            .p_2()
            .cursor_pointer()
            .child("Clear selection")
            .on_click(cx.listener(|this, _event, window, cx| {
                this.focus_handle.focus(window);
                this.select_connection(None, window, cx);
                eprintln!("[kafka-studio] event=selection_cleared source=mouse");
            })),
    );
    list
}

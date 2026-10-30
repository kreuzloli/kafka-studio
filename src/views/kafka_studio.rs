//! 只读客户端的 Demo 外壳：连接选择、目录折叠与键盘导航。

use std::collections::HashSet;

use gpui::{
    App, Div, Entity, FocusHandle, KeyDownEvent, ScrollHandle, Window, div, prelude::*, px, rgb,
    rgba,
};
use gpui_component::input::{InputState, TextInput};

use crate::{
    components::selectable_text,
    enums::ConnectionTreeRow,
    models::{Connection, ConnectionFolder, ConnectionFolderId},
    views::connection_tree,
};

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
                },
                Connection {
                    id: uuid::Uuid::new_v4().to_string(),
                    name: String::from("Test Demo"),
                    bootstrap_servers: String::from("test.example:9092"),
                    folder_id: None,
                    credentials: None,
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

    /// 添加当前会话的连接配置，生成未被占用的默认名称并选中新行。
    fn add_connection(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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
            folder_id: None,
            credentials: None,
        });

        self.expanded = true;
        self.select_connection(Some(index), window, cx);
        eprintln!("[kafka-studio] event=connection_added index={index}");
    }

    /// 删除选中配置并清空草稿，避免继续使用删除前的索引。
    fn delete_connection(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(index) = self.selected_connection else {
            return;
        };
        self.connections.remove(index);
        self.select_connection(None, window, cx);
        eprintln!("[kafka-studio] event=connection_deleted index={index}");
    }
}

impl Render for KafkaStudio {
    /// 根据当前状态构建布局；渲染可能频繁发生，因此这里不输出交互日志。
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let selected_connection = self
            .selected_connection
            .map(|index| &self.connections[index]);
        div()
            .track_focus(&self.focus_handle)
            // 传入方法而非调用方法，由 listener 在事件到来时取得可变实体。
            .on_key_down(cx.listener(Self::handle_key_down))
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
                            .child(connection_list(cx, self.selected_connection, self.expanded, &self.connections, &self.sidebar_scroll, &self.folders, &self.collapsed_folders))
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_w_0()
                            .child(connection_workspace(selected_connection, &self.folders, window, cx))
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap_2()
                                    .p_6()
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap_1()
                                            .p_2()
                                            .child(
                                                div()
                                                    .w(px(180.0))
                                                    .flex_shrink_0()
                                                    .child(selectable_text(
                                                        "name-label",
                                                        "Connection name:",
                                                        window,
                                                        cx,
                                                    )),
                                            )
                                            .child(TextInput::new(&self.name_input)),
                                    )
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap_1()
                                            .p_2()
                                            .child(
                                                div()
                                                    .w(px(180.0))
                                                    .flex_shrink_0()
                                                    .child(selectable_text(
                                                        "bootstrap-label",
                                                        "Bootstrap servers:",
                                                        window,
                                                        cx,
                                                    )),
                                            )
                                            .child(TextInput::new(&self.bootstrap_input)),
                                    )
                                    .child(div().flex().flex_row().gap_1().p_2()
                                    .child(div().id("reset-fields").p_2().cursor_pointer().child("Reset fields").on_click(cx.listener(|this, _event, window, cx|{
                                        let selected = this.selected_connection;
                                        this.select_connection(selected, window, cx);
                                        eprintln!(
                                            "[kafka-studio] event=connection_draft_reset selected={selected:?}"
                                        );
                                    })))
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
                                    )
                                    .when_some(self.form_error, |this, message| {
                                        this.child(selectable_text("form-error", message, window, cx))
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
) -> Div {
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
                .id("add-connection")
                .flex_shrink_0()
                .p_2()
                .cursor_pointer()
                .child("+ New connection")
                .on_click(cx.listener(|this, _event, window, cx| {
                    this.add_connection(window, cx);
                })),
        )
        .child(
            div()
                .id("delete-connection")
                .flex_shrink_0()
                .p_2()
                .cursor_pointer()
                .child("- Delete selected")
                .on_click(cx.listener(|this, _event, window, cx| {
                    this.delete_connection(window, cx);
                })),
        )
        .child(
            div()
                .id("folder")
                .flex_shrink_0()
                .p_2()
                .cursor_pointer()
                .child(if expanded {
                    "▼ All connections"
                } else {
                    "▶ All connections"
                })
                .on_click(cx.listener(|this, _event, window, cx| {
                    this.focus_handle.focus(window);
                    this.expanded = !this.expanded;
                    eprintln!(
                        "[kafka-studio] event=folder_clicked expanded={}",
                        this.expanded
                    );
                    cx.notify();
                })),
        );
    if expanded {
        let mut connection_rows = div()
            .id("connection-rows-scroll")
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .pl(px(16.0))
            .overflow_y_scroll()
            .track_scroll(scroll_handle);
        let rows = connection_tree::build_connection_rows(folders, connections, collapsed_folders);
        if rows.is_empty() {
            connection_rows = connection_rows.child(
                div()
                    .p_2()
                    .text_color(rgb(0x94a3b8))
                    .child("No connections. Click + New connection."),
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
                    let collapsed = collapsed_folders.contains(&folder_id);
                    let arrow = if collapsed { "▶" } else { "▼" };
                    connection_rows = connection_rows.child(
                        div()
                            .id(("connection-folder", folder_index))
                            .flex_shrink_0()
                            .p_2()
                            .pl(px(8.0 + depth as f32 * 16.0))
                            .text_color(rgb(0x94a3b8))
                            .cursor_pointer()
                            .child(format!("{arrow} {}", folder.name))
                            .on_click(cx.listener(move |this, _event, window, cx| {
                                this.focus_handle.focus(window);
                                if !this.collapsed_folders.remove(&folder_id) {
                                    this.collapsed_folders.insert(folder_id.clone());
                                }
                                eprintln!(
                                    "[kafka-studio] event=folder_toggled id={:?} collapsed={}",
                                    folder_id,
                                    this.collapsed_folders.contains(&folder_id)
                                );
                                cx.notify();
                            })),
                    );
                }
                ConnectionTreeRow::Connection {
                    connection_index,
                    depth,
                } => {
                    let connection = &connections[connection_index];
                    connection_rows = connection_rows.child(
                        div()
                            .id(("connection", connection_index))
                            .flex_shrink_0()
                            .bg(if selected_connection == Some(connection_index) {
                                rgba(0x334155e6)
                            } else {
                                rgba(0x18181be6)
                            })
                            .p_2()
                            .pl(px(8.0 + depth as f32 * 16.0))
                            .cursor_pointer()
                            .child(connection.name.clone())
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

/// 展示已保存配置，独立于编辑草稿；详情及空状态均允许选择复制。
pub(super) fn connection_workspace(
    conection: Option<&Connection>,
    folders: &[ConnectionFolder],
    window: &mut Window,
    cx: &mut App,
) -> Div {
    let content = match conection {
        None => div().child(selectable_text(
            "workspace-empty",
            "Select a connection",
            window,
            cx,
        )),
        Some(conection) => {
            let folder_name = match conection.folder_id.as_ref() {
                None => "Root",
                Some(folder_id) => folders
                    .iter()
                    .find(|f| &f.id == folder_id)
                    .map(|f| f.name.as_str())
                    .unwrap_or("Unknown folder"),
            };
            div()
                .flex()
                .flex_col()
                .gap_2()
                .child(selectable_text(
                    "workspace-folder",
                    format!("Folder: {folder_name}"),
                    window,
                    cx,
                ))
                .child(selectable_text(
                    "workspace-name",
                    conection.name.clone(),
                    window,
                    cx,
                ))
                .child(selectable_text(
                    "workspace-bootstrap",
                    format!("Bootstrap servers: {}", conection.bootstrap_servers),
                    window,
                    cx,
                ))
                .child(selectable_text(
                    "workspace-status",
                    "Status: not connected",
                    window,
                    cx,
                ))
        }
    };
    // 只读输入框依赖父容器宽度，不会像普通文字一样撑开自动宽度的容器。
    div()
        .flex()
        .flex_1()
        .min_w_0()
        .p_6()
        .child(content.w_full().min_w_0())
}

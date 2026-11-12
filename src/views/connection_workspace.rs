use gpui::{App, Div, Window, div, prelude::*};
use gpui_component::button::Button;

use crate::{
    components::selectable_text,
    models::{Connection, ConnectionFolder, ConnectionSecurity},
};

/// 根据导入来源提示证书迁移事项；不访问文件，也不判断 TLS 是否有效。
pub(super) fn certificate_hint(conn: &Connection) -> Option<&'static str> {
    if conn.ca_location.is_some() {
        return None;
    }
    if let Some(truststore_location) = conn.truststore_location.as_deref() {
        let truststore_location = truststore_location.trim();
        if truststore_location.is_empty() {
            return None;
        }
        if truststore_location.to_ascii_lowercase().ends_with(".jks") {
            return Some(
                "Certificate migration pending: export the JKS CA certificates to PEM, then select the local CA file.",
            );
        } else {
            return Some(
                "Certificate mapping pending: select the corresponding local PEM CA file.",
            );
        }
    }
    None
}

/// 展示已保存配置，独立于编辑草稿；详情及空状态均允许选择复制。
pub(super) fn connection_workspace(
    connection: Option<&Connection>,
    run_fast_query: Button,
    run_slow_query: Button,
    cancel_fake_query: Button,
    read_metadata: Button,
    metadata_error: Option<&'static str>,
    metadata_result: Option<&str>,
    cancel_metadata: Button,
    metadata_pending: bool,
    folders: &[ConnectionFolder],
    fake_query_pending: bool,
    fake_query_result: Option<&'static str>,
    window: &mut Window,
    cx: &mut App,
) -> Div {
    let content = match connection {
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
            let ca_description = match conection.ca_location.as_deref() {
                Some(path) => format!(
                    "CA file: {} (selected for metadata requests)",
                    path.display()
                ),
                None => String::from("CA file: not selected"),
            };
            let truststore_description = match conection.truststore_location.as_deref() {
                Some(path) => format!("Imported truststore: {path}"),
                None => String::from("Imported truststore: not provided"),
            };
            let security_description = match conection.security {
                Some(ConnectionSecurity::SaslSslScramSha512) => {
                    "Security: SASL_SSL / SCRAM-SHA-512"
                }
                None => "Security: not specified",
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
                    "workspace-security",
                    security_description,
                    window,
                    cx,
                ))
                .child(selectable_text(
                    "workspace-truststore",
                    truststore_description,
                    window,
                    cx,
                ))
                .child(selectable_text("workspace-ca", ca_description, window, cx))
                .when_some(certificate_hint(conection), |this, message| {
                    this.child(selectable_text(
                        "workspace-certificate-hint",
                        message,
                        window,
                        cx,
                    ))
                })
                .child(selectable_text(
                    "workspace-status",
                    "Mode: one-shot metadata requests",
                    window,
                    cx,
                ))
                .child(
                    div()
                        .flex()
                        .gap_2()
                        .child(run_slow_query)
                        .child(run_fast_query)
                        .child(read_metadata)
                        .when(metadata_pending, |this| this.child(cancel_metadata))
                        .when(fake_query_pending, |this| this.child(cancel_fake_query)),
                )
                .when_some(metadata_error, |this, message| {
                    this.child(selectable_text(
                        "workspace-metadata-error",
                        message,
                        window,
                        cx,
                    ))
                })
                .when_some(metadata_result, |this, message| {
                    this.child(selectable_text(
                        "workspace-metadata-result",
                        message.to_owned(),
                        window,
                        cx,
                    ))
                })
                .when(fake_query_pending, |this| {
                    this.child("Waiting for Fake query…")
                })
                .when_some(fake_query_result, |this, message| this.child(message))
                .child(div().flex().flex_col().gap_2().mt_4().child("Topic").child(
                    selectable_text(
                        "workspace-topics-placeholder",
                        "Topic list is not loaded yet.",
                        window,
                        cx,
                    ),
                ))
        }
    };
    // 只读输入框依赖父容器宽度，不会像普通文字一样撑开自动宽度的容器。
    div()
        .flex()
        .flex_shrink_0()
        .min_w_0()
        .p_6()
        .child(content.w_full().min_w_0())
}

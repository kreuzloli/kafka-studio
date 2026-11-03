use crate::{
    enums::ImportError,
    models::{Connection, ConnectionFolder},
};

/// 已完成转换、尚未加入当前列表的导入数据。
#[derive(Debug)]
pub(crate) struct ImportPreview {
    pub(crate) folders: Vec<ConnectionFolder>,
    pub(crate) connections: Vec<Connection>,
}

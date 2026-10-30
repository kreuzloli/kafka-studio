//! 界面模块：对外暴露根视图，内部布局函数保留在实现模块中。

mod kafka_studio;
pub use kafka_studio::KafkaStudio;
mod connection_tree;

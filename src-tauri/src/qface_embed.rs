// 嵌入 QQ 表情 GIF 文件
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "../third/QFace/public/gif"]
#[prefix = "gif/"]
pub struct QFaceGif;


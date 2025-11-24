// 模块声明
mod runbot;
mod storage;
mod avatar;
mod image;
mod qface_embed;

use std::sync::{Arc, Mutex, OnceLock};
use runbot::RunbotState;
use tauri::Manager;

// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

static APP_DATA_DIR: OnceLock<String> = OnceLock::new();
static CURRENT_SELF_ID: OnceLock<Arc<Mutex<Option<i64>>>> = OnceLock::new();

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // 初始化 tracing subscriber
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info,runbot_desktop_lib=debug,runbot=debug"))
        )
        .with_target(false) // 不显示目标模块名（可选）
        .with_thread_ids(false) // 不显示线程 ID（可选）
        .with_file(false) // 不显示文件名（可选）
        .with_line_number(false) // 不显示行号（可选）
        .init();
    
    tracing::info!("初始化 Tauri 应用");
    
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .register_uri_scheme_protocol("asset", move |_app_handle, request| {
                let uri_str = request.uri().to_string();
                
                // 处理头像请求：asset://avatar/user/123456.png 或 asset://avatar/group/123456.png
                if uri_str.starts_with("asset://avatar/") {
                    // 解析路径：asset://avatar/user/123456.png -> user/123456.png
                    let path_part = uri_str.strip_prefix("asset://avatar/")
                        .unwrap_or("");
                    
                    let parts: Vec<&str> = path_part.split('/').collect();
                    if parts.len() != 2 {
                        return tauri::http::Response::builder()
                            .status(400)
                            .body("Invalid avatar URL format".as_bytes().to_vec())
                            .unwrap();
                    }
                    
                    let avatar_type = parts[0]; // "user" 或 "group"
                    let id_str = parts[1]; // "123456.png"
                    
                    // 提取 ID（去掉扩展名）
                    let id = match id_str.split('.').next() {
                        Some(id) => match id.parse::<i64>() {
                            Ok(id) => id,
                            Err(_) => {
                                return tauri::http::Response::builder()
                                    .status(400)
                                    .body("Invalid avatar ID".as_bytes().to_vec())
                                    .unwrap();
                            }
                        },
                        None => {
                            return tauri::http::Response::builder()
                                .status(400)
                                .body("Invalid avatar ID format".as_bytes().to_vec())
                                .unwrap();
                        }
                    };
                    
                    // 获取 self_id（从静态变量获取）
                    let self_id: Option<i64> = CURRENT_SELF_ID
                        .get()
                        .and_then(|id| id.lock().ok())
                        .and_then(|id| *id);
                    
                    // 获取应用数据目录
                    let app_data_dir_str = match APP_DATA_DIR.get() {
                        Some(dir) => dir.clone(),
                        None => {
                            return tauri::http::Response::builder()
                                .status(500)
                                .body("App data directory not initialized".as_bytes().to_vec())
                                .unwrap();
                        }
                    };
                    
                    // 使用同步方式处理头像（在协议处理器中）
                    // 使用同步函数，避免创建新的 runtime
                    match avatar::get_user_avatar_sync(id, self_id, &app_data_dir_str, avatar_type == "group") {
                        Ok(Some(relative_path)) => {
                            let full_path = std::path::Path::new(&app_data_dir_str).join(&relative_path);
                            
                            match std::fs::read(&full_path) {
                                Ok(data) => {
                                    let mime_type = "image/png";
                                    tauri::http::Response::builder()
                                        .status(200)
                                        .header("Content-Type", mime_type)
                                        .body(data)
                                        .unwrap()
                                }
                                Err(e) => {
                                    tauri::http::Response::builder()
                                        .status(500)
                                        .body(format!("File read error: {}", e).into_bytes())
                                        .unwrap()
                                }
                            }
                        }
                        Ok(None) => {
                            tauri::http::Response::builder()
                                .status(404)
                                .body("Avatar not found".as_bytes().to_vec())
                                .unwrap()
                        }
                        Err(e) => {
                            tauri::http::Response::builder()
                                .status(500)
                                .body(format!("Avatar error: {}", e).into_bytes())
                                .unwrap()
                        }
                    }
                } else if uri_str.starts_with("asset://qface/") {
                    // 处理表情文件请求：asset://qface/gif/s123.gif 或 asset://qface/static/s123.png
                    let path_part = uri_str.strip_prefix("asset://qface/")
                        .unwrap_or("");
                    
                    // 只处理 gif 文件
                    if !path_part.starts_with("gif/") || !path_part.ends_with(".gif") {
                        return tauri::http::Response::builder()
                            .status(404)
                            .body("Only GIF faces are embedded".as_bytes().to_vec())
                            .unwrap();
                    }
                    
                    // 从嵌入的资源中读取文件
                    // path_part 格式：gif/s123.gif
                    let file_path = path_part; // 已经是 gif/s123.gif 格式
                    
                    match qface_embed::QFaceGif::get(file_path) {
                        Some(file) => {
                            tauri::http::Response::builder()
                                .status(200)
                                .header("Content-Type", "image/gif")
                                .header("Cache-Control", "public, max-age=31536000")
                                .body(file.data.to_vec())
                                .unwrap()
                        }
                        None => {
                            tracing::debug!("表情文件未找到: {}", file_path);
                            tauri::http::Response::builder()
                                .status(404)
                                .body(format!("Face file not found: {}", file_path).as_bytes().to_vec())
                                .unwrap()
                        }
                    }
                } else if uri_str.starts_with("asset://localhost/") {
                    // 兼容旧的格式：asset://localhost/user_xxx/avatars/xxx.png
                    let path_part = uri_str.strip_prefix("asset://localhost/")
                        .unwrap_or(&uri_str);
                    
                    let decoded_path = match urlencoding::decode(path_part) {
                        Ok(decoded) => decoded.to_string(),
                        Err(_) => path_part.to_string(),
                    };
                    
                    if !decoded_path.starts_with("user_") {
                        return tauri::http::Response::builder()
                            .status(403)
                            .body(format!("Access denied: invalid path (must start with 'user_'): {}", decoded_path).as_bytes().to_vec())
                            .unwrap();
                    }
                    
                    let app_data_dir_str = match APP_DATA_DIR.get() {
                        Some(dir) => dir,
                        None => {
                            return tauri::http::Response::builder()
                                .status(500)
                                .body("App data directory not initialized".as_bytes().to_vec())
                                .unwrap();
                        }
                    };
                    
                    let full_path = std::path::Path::new(app_data_dir_str).join(&decoded_path);
                    
                    if !full_path.starts_with(app_data_dir_str) {
                        return tauri::http::Response::builder()
                            .status(403)
                            .body("Access denied: path traversal detected".as_bytes().to_vec())
                            .unwrap();
                    }
                    
                    if !full_path.exists() {
                        return tauri::http::Response::builder()
                            .status(404)
                            .body(format!("File not found: {}", decoded_path).into_bytes())
                            .unwrap();
                    }
                    
                    match std::fs::read(&full_path) {
                        Ok(data) => {
                            let mime_type = if decoded_path.ends_with(".png") {
                                "image/png"
                            } else if decoded_path.ends_with(".jpg") || decoded_path.ends_with(".jpeg") {
                                "image/jpeg"
                            } else if decoded_path.ends_with(".gif") {
                                "image/gif"
                            } else if decoded_path.ends_with(".webp") {
                                "image/webp"
                            } else {
                                "application/octet-stream"
                            };
                            
                            tauri::http::Response::builder()
                                .status(200)
                                .header("Content-Type", mime_type)
                                .body(data)
                                .unwrap()
                        }
                        Err(e) => {
                            tauri::http::Response::builder()
                                .status(500)
                                .body(format!("File read error: {}", e).into_bytes())
                                .unwrap()
                        }
                    }
                } else {
                    // 默认的 asset 协议处理（让 Tauri 处理前端资源）
                    tauri::http::Response::builder()
                        .status(404)
                        .body("Not found".as_bytes().to_vec())
                        .unwrap()
                }
            })
        .setup(|app| {
            // 初始化应用数据目录到静态变量
            let app_data_dir = app.path().app_data_dir().unwrap();
            let app_data_dir_str = app_data_dir.to_string_lossy().to_string();
            APP_DATA_DIR.set(app_data_dir_str).expect("Failed to set app data dir");
            
            // 初始化 self_id 静态变量
            CURRENT_SELF_ID.set(Arc::new(Mutex::new(None))).expect("Failed to set CURRENT_SELF_ID");
            
            Ok(())
        })
        .manage(Arc::new(Mutex::new(RunbotState::default())))
        .invoke_handler(tauri::generate_handler![
            greet,
            // Runbot 命令
            runbot::connect_runbot,
            runbot::disconnect_runbot,
            runbot::get_runbot_status,
            runbot::get_runbot_self_id,
            runbot::send_runbot_message,
            // 存储命令
            storage::save_config,
            storage::load_config,
            storage::remove_config,
            storage::save_message,
            storage::update_message_id,
            storage::update_message_content,
            storage::get_messages,
            storage::search_messages,
            storage::delete_message,
            storage::cleanup_old_messages,
            storage::get_message_stats,
            // 头像命令
            avatar::get_user_avatar,
            avatar::get_group_avatar,
            avatar::clear_avatar_cache,
            // 图片命令
            image::check_image_cache,
            image::download_image,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

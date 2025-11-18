use tauri::{AppHandle, Manager};
use std::path::{Path, PathBuf};
use std::fs;
use std::io::Write;
use sha2::{Sha256, Digest};
use base64::{Engine as _, engine::general_purpose};
use std::time::Duration;
use tracing;
use serde_json;

/// 获取图片缓存目录
fn get_image_cache_dir(app: &AppHandle, self_id: Option<i64>) -> Result<PathBuf, String> {
    let mut path = app.path()
        .app_data_dir()
        .map_err(|e| format!("获取应用数据目录失败: {}", e))?;
    
    fs::create_dir_all(&path)
        .map_err(|e| format!("创建应用数据目录失败: {}", e))?;
    
    if let Some(uid) = self_id {
        path.push(format!("user_{}", uid));
        fs::create_dir_all(&path)
            .map_err(|e| format!("创建用户数据目录失败: {}", e))?;
    }
    
    path.push("images");
    fs::create_dir_all(&path)
        .map_err(|e| format!("创建图片缓存目录失败: {}", e))?;
    
    Ok(path)
}

/// 生成图片文件名（使用 URL 的 SHA256 哈希）
fn get_image_filename(url: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(url.as_bytes());
    let hash = hasher.finalize();
    let hash_str = general_purpose::STANDARD.encode(hash);
    
    // 尝试从 URL 获取文件扩展名
    let ext = if let Some(dot_pos) = url.rfind('.') {
        if let Some(query_pos) = url[dot_pos..].find('?') {
            &url[dot_pos + 1..dot_pos + query_pos]
        } else {
            &url[dot_pos + 1..]
        }
    } else {
        "jpg" // 默认扩展名
    };
    
    // 限制扩展名长度
    let ext = if ext.len() > 10 { "jpg" } else { ext };
    
    format!("{}.{}", &hash_str[..16], ext)
}

/// 下载图片并保存到缓存（同步版本）
fn download_image_sync(url: &str, cache_path: &Path) -> Result<(), String> {
    tracing::debug!("[download_image_sync] 开始下载图片: URL = {}", url);
    tracing::debug!("[download_image_sync] 缓存路径: {:?}", cache_path);
    
    // 使用 reqwest 的阻塞客户端
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(30))
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36")
        .build()
        .map_err(|e| {
            tracing::error!("[download_image_sync] 创建 HTTP 客户端失败: {}", e);
            format!("创建 HTTP 客户端失败: {}", e)
        })?;
    
    tracing::debug!("[download_image_sync] 发送 HTTP GET 请求...");
    let response = client
        .get(url)
        .send()
        .map_err(|e| {
            tracing::error!("[download_image_sync] 发送请求失败: {}", e);
            format!("下载图片失败: {}", e)
        })?;
    
    let status = response.status();
    tracing::debug!("[download_image_sync] HTTP 响应状态: {}", status);
    
    if !status.is_success() {
        // 尝试读取响应体以获取更多错误信息
        let error_body = response.text().unwrap_or_else(|_| "无法读取响应体".to_string());
        tracing::error!("[download_image_sync] HTTP 错误响应 ({}): {}", status, error_body);
        return Err(format!("下载图片失败: HTTP {} - {}", status, error_body));
    }
    
    tracing::debug!("[download_image_sync] 读取响应数据...");
    let bytes = response
        .bytes()
        .map_err(|e| {
            tracing::error!("[download_image_sync] 读取图片数据失败: {}", e);
            format!("读取图片数据失败: {}", e)
        })?;
    
    tracing::debug!("[download_image_sync] 图片大小: {} 字节", bytes.len());
    
    // 确保父目录存在
    if let Some(parent) = cache_path.parent() {
        tracing::debug!("[download_image_sync] 创建父目录: {:?}", parent);
        fs::create_dir_all(parent)
            .map_err(|e| {
                tracing::error!("[download_image_sync] 创建图片缓存目录失败: {}", e);
                format!("创建图片缓存目录失败: {}", e)
            })?;
    }
    
    // 保存到文件
    tracing::debug!("[download_image_sync] 保存文件到: {:?}", cache_path);
    let mut file = fs::File::create(cache_path)
        .map_err(|e| {
            tracing::error!("[download_image_sync] 创建图片文件失败: {}", e);
            format!("创建图片文件失败: {}", e)
        })?;
    
    file.write_all(&bytes)
        .map_err(|e| {
            tracing::error!("[download_image_sync] 写入图片文件失败: {}", e);
            format!("写入图片文件失败: {}", e)
        })?;
    
    tracing::info!("[download_image_sync] 图片下载成功: {:?}", cache_path);
    Ok(())
}

/// 检查图片缓存
#[tauri::command]
pub async fn check_image_cache(
    url: String,
    self_id: Option<i64>,
    app: AppHandle,
) -> Result<Option<String>, String> {
    tracing::debug!("[check_image_cache] 检查缓存: URL = {}, self_id = {:?}", url, self_id);
    
    let app_data_dir = app.path()
        .app_data_dir()
        .map_err(|e| format!("获取应用数据目录失败: {}", e))?;
    
    let cache_dir = get_image_cache_dir(&app, self_id)?;
    tracing::debug!("[check_image_cache] 缓存目录: {:?}", cache_dir);
    
    let filename = get_image_filename(&url);
    let cache_path = cache_dir.join(&filename);
    tracing::debug!("[check_image_cache] 缓存文件路径: {:?}", cache_path);
    
    // 检查缓存是否存在且未过期（30天）
    if cache_path.exists() {
        if let Ok(metadata) = fs::metadata(&cache_path) {
            if let Ok(modified) = metadata.modified() {
                let age = std::time::SystemTime::now()
                    .duration_since(modified)
                    .unwrap_or_default();
                
                // 如果缓存未过期（30天），返回相对路径
                if age.as_secs() < 30 * 24 * 60 * 60 {
                    let relative_path = cache_path.strip_prefix(&app_data_dir)
                        .map_err(|_| "无法计算相对路径".to_string())?;
                    tracing::debug!("[check_image_cache] 缓存命中: {:?}", relative_path);
                    return Ok(relative_path.to_str().map(|s| s.to_string()));
                } else {
                    tracing::debug!("[check_image_cache] 缓存已过期");
                }
            }
        }
    } else {
        tracing::debug!("[check_image_cache] 缓存不存在");
    }
    
    Ok(None)
}

/// 下载图片并缓存
#[tauri::command]
pub async fn download_image(
    url: String,
    self_id: Option<i64>,
    file: Option<String>, // 图片文件标识符，用于 URL 过期时重新获取
    app: AppHandle,
    state: tauri::State<'_, std::sync::Arc<std::sync::Mutex<crate::runbot::RunbotState>>>,
) -> Result<Option<String>, String> {
    tracing::info!("[download_image] 开始下载图片: URL = {}, self_id = {:?}", url, self_id);
    
    let app_data_dir = app.path()
        .app_data_dir()
        .map_err(|e| {
            tracing::error!("[download_image] 获取应用数据目录失败: {}", e);
            format!("获取应用数据目录失败: {}", e)
        })?;
    
    tracing::debug!("[download_image] 应用数据目录: {:?}", app_data_dir);
    
    let cache_dir = get_image_cache_dir(&app, self_id)?;
    tracing::debug!("[download_image] 缓存目录: {:?}", cache_dir);
    
    let filename = get_image_filename(&url);
    tracing::debug!("[download_image] 生成的文件名: {}", filename);
    
    let cache_path = cache_dir.join(&filename);
    tracing::debug!("[download_image] 完整缓存路径: {:?}", cache_path);
    
    // 先检查缓存
    if cache_path.exists() {
        if let Ok(metadata) = fs::metadata(&cache_path) {
            if let Ok(modified) = metadata.modified() {
                let age = std::time::SystemTime::now()
                    .duration_since(modified)
                    .unwrap_or_default();
                
                // 如果缓存未过期（30天），返回相对路径
                if age.as_secs() < 30 * 24 * 60 * 60 {
                    let relative_path = cache_path.strip_prefix(&app_data_dir)
                        .map_err(|_| "无法计算相对路径".to_string())?;
                    tracing::info!("[download_image] 使用缓存: {:?}", relative_path);
                    return Ok(relative_path.to_str().map(|s| s.to_string()));
                } else {
                    tracing::debug!("[download_image] 缓存已过期，需要重新下载");
                }
            }
        }
    } else {
        tracing::debug!("[download_image] 缓存不存在，需要下载");
    }
    
    // 下载图片（使用同步方式，因为这是异步函数）
    // 使用 tokio::task::spawn_blocking 在后台线程执行阻塞操作
    let url_clone = url.clone();
    let cache_path_clone = cache_path.clone();
    let result = tokio::task::spawn_blocking(move || {
        download_image_sync(&url_clone, &cache_path_clone)
    })
    .await
    .map_err(|e| {
        tracing::error!("[download_image] 执行下载任务失败: {}", e);
        format!("执行下载任务失败: {}", e)
    })?;
    
    match result {
        Ok(_) => {
            let relative_path = cache_path.strip_prefix(&app_data_dir)
                .map_err(|_| "无法计算相对路径".to_string())?;
            tracing::info!("[download_image] 下载成功: {:?}", relative_path);
            Ok(relative_path.to_str().map(|s| s.to_string()))
        }
        Err(e) => {
            tracing::warn!("[download_image] 下载图片失败: {}", e);
            
            // 如果下载失败（可能是 URL 过期），尝试通过 get_image_detail API 获取新的 URL
            if let Some(file_id) = &file {
                // 检查错误是否是 HTTP 400/403/404（URL 可能过期）
                let is_url_expired = e.contains("HTTP 400") || e.contains("HTTP 403") || e.contains("HTTP 404");
                
                if is_url_expired {
                    tracing::info!("[download_image] URL 可能已过期，尝试通过 get_image_detail API 获取新 URL: file = {}", file_id);
                    
                    // 获取 bot_ctx
                    let bot_ctx = {
                        let state_guard = state.lock().map_err(|e| format!("锁定状态失败: {}", e))?;
                        state_guard.bot_ctx.clone()
                    };
                    
                    if let Some(bot_ctx) = bot_ctx {
                        // 直接调用异步 API（因为我们在 async 函数中）
                        // NapCat 使用 get_image API，它返回包含 url 的完整数据
                        let file_id_clone = file_id.clone();
                        let new_url_result = {
                            // 先尝试 get_image_detail
                            match bot_ctx.get_image_detail(&file_id_clone).await {
                                Ok(detail) => {
                                    tracing::info!("[download_image] 通过 get_image_detail 成功获取新 URL: {}", detail.url);
                                    Ok(detail.url)
                                }
                                Err(_) => {
                                    // 如果 get_image_detail 失败，尝试使用 get_image API
                                    // 根据 NapCat API 文档，get_image 返回的数据包含 url 字段
                                    tracing::info!("[download_image] get_image_detail 不可用，尝试 get_image API");
                                    let response = bot_ctx
                                        .websocket_send("get_image", serde_json::json!({ "file": file_id_clone }))
                                        .await
                                        .map_err(|e| format!("调用 get_image API 失败: {:?}", e))?;
                                    
                                    let data = response.data(tokio::time::Duration::from_secs(10))
                                        .await
                                        .map_err(|e| format!("获取 get_image 响应失败: {:?}", e))?;
                                    
                                    // 手动解析 JSON 获取 url 字段
                                    if let Some(url) = data.get("url").and_then(|v| v.as_str()) {
                                        tracing::info!("[download_image] 通过 get_image 成功获取新 URL: {}", url);
                                        Ok(url.to_string())
                                    } else {
                                        tracing::error!("[download_image] get_image 返回的数据中没有 url 字段: {:?}", data);
                                        Err("get_image 返回的数据中没有 url 字段".to_string())
                                    }
                                }
                            }
                        };
                        
                        if let Ok(new_url) = new_url_result {
                            tracing::info!("[download_image] 使用新 URL 重新下载: {}", new_url);
                            
                            // 使用新 URL 重新下载
                            let new_url_clone = new_url.clone();
                            let cache_path_clone = cache_path.clone();
                            let retry_result = tokio::task::spawn_blocking(move || {
                                download_image_sync(&new_url_clone, &cache_path_clone)
                            })
                            .await
                            .map_err(|e| format!("执行重试下载任务失败: {}", e))?;
                            
                            match retry_result {
                                Ok(_) => {
                                    let relative_path = cache_path.strip_prefix(&app_data_dir)
                                        .map_err(|_| "无法计算相对路径".to_string())?;
                                    tracing::info!("[download_image] 使用新 URL 下载成功: {:?}", relative_path);
                                    return Ok(relative_path.to_str().map(|s| s.to_string()));
                                }
                                Err(retry_err) => {
                                    tracing::error!("[download_image] 使用新 URL 下载仍然失败: {}", retry_err);
                                }
                            }
                        }
                    } else {
                        tracing::warn!("[download_image] bot_ctx 不可用，无法获取新 URL");
                    }
                }
            }
            
            // 如果下载失败但缓存存在，返回相对路径
            if cache_path.exists() {
                tracing::warn!("[download_image] 下载失败，但使用旧缓存");
                let relative_path = cache_path.strip_prefix(&app_data_dir)
                    .map_err(|_| "无法计算相对路径".to_string())?;
                Ok(relative_path.to_str().map(|s| s.to_string()))
            } else {
                Ok(None)
            }
        }
    }
}


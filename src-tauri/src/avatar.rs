use tauri::{AppHandle, Manager};
use std::path::{Path, PathBuf};
use std::fs;
use std::io::Write;
use sha2::{Sha256, Digest};
use base64::{Engine as _, engine::general_purpose};
use std::time::Duration;

/// 获取用户数据目录（根据 self_id）
fn get_user_data_dir(app: &AppHandle, self_id: Option<i64>) -> Result<PathBuf, String> {
    let dir = app.path()
        .app_data_dir()
        .map_err(|e| format!("获取应用数据目录失败: {}", e))?;
    
    fs::create_dir_all(&dir)
        .map_err(|e| format!("创建应用数据目录失败: {}", e))?;
    
    let mut path = dir;
    if let Some(uid) = self_id {
        path.push(format!("user_{}", uid));
        fs::create_dir_all(&path)
            .map_err(|e| format!("创建用户数据目录失败: {}", e))?;
    }
    
    Ok(path)
}

/// 获取头像缓存目录
fn get_avatar_cache_dir(app: &AppHandle, self_id: Option<i64>) -> Result<PathBuf, String> {
    let mut path = get_user_data_dir(app, self_id)?;
    path.push("avatars");
    fs::create_dir_all(&path)
        .map_err(|e| format!("创建头像缓存目录失败: {}", e))?;
    Ok(path)
}

/// 生成头像文件名（使用 SHA256 哈希）
fn get_avatar_filename(user_id: i64, is_group: bool) -> String {
    let mut hasher = Sha256::new();
    let prefix = if is_group { "group" } else { "user" };
    hasher.update(format!("{}:{}", prefix, user_id));
    let hash = hasher.finalize();
    let hash_str = general_purpose::STANDARD.encode(hash);
    format!("{}_{}.png", prefix, &hash_str[..16])
}

/// 下载头像并保存到缓存
async fn download_avatar(url: &str, cache_path: &Path) -> Result<(), String> {
    // 使用 reqwest 下载头像
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| format!("创建 HTTP 客户端失败: {}", e))?;
    
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|e| format!("下载头像失败: {}", e))?;
    
    if !response.status().is_success() {
        return Err(format!("下载头像失败: HTTP {}", response.status()));
    }
    
    let bytes = response
        .bytes()
        .await
        .map_err(|e| format!("读取头像数据失败: {}", e))?;
    
    // 确保父目录存在
    if let Some(parent) = cache_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("创建头像缓存目录失败: {}", e))?;
    }
    
    // 保存到文件
    let mut file = fs::File::create(cache_path)
        .map_err(|e| format!("创建头像文件失败: {}", e))?;
    
    file.write_all(&bytes)
        .map_err(|e| format!("写入头像文件失败: {}", e))?;
    
    Ok(())
}

/// 获取用户头像（带缓存）
/// 返回相对于应用数据目录的相对路径，例如：user_956279803/avatars/user_xxx.png
#[tauri::command]
pub async fn get_user_avatar(
    user_id: i64,
    self_id: Option<i64>,
    app: AppHandle,
) -> Result<Option<String>, String> {
    let app_data_dir = app.path()
        .app_data_dir()
        .map_err(|e| format!("获取应用数据目录失败: {}", e))?;
    
    let cache_dir = get_avatar_cache_dir(&app, self_id)?;
    let filename = get_avatar_filename(user_id, false);
    let cache_path = cache_dir.join(&filename);
    
    // 检查缓存是否存在且未过期（7天）
    if cache_path.exists() {
        if let Ok(metadata) = fs::metadata(&cache_path) {
            if let Ok(modified) = metadata.modified() {
                let age = std::time::SystemTime::now()
                    .duration_since(modified)
                    .unwrap_or_default();
                
                // 如果缓存未过期（7天），返回相对路径
                if age.as_secs() < 7 * 24 * 60 * 60 {
                    let relative_path = cache_path.strip_prefix(&app_data_dir)
                        .map_err(|_| "无法计算相对路径".to_string())?;
                    return Ok(relative_path.to_str().map(|s| s.to_string()));
                }
            }
        }
    }
    
    // 下载头像
    let url = format!("http://q.qlogo.cn/headimg_dl?dst_uin={}&spec=640&img_type=png", user_id);
    
    match download_avatar(&url, &cache_path).await {
        Ok(_) => {
            let relative_path = cache_path.strip_prefix(&app_data_dir)
                .map_err(|_| "无法计算相对路径".to_string())?;
            Ok(relative_path.to_str().map(|s| s.to_string()))
        }
        Err(e) => {
            eprintln!("下载头像失败: {}", e);
            // 如果下载失败但缓存存在，返回相对路径
            if cache_path.exists() {
                let relative_path = cache_path.strip_prefix(&app_data_dir)
                    .map_err(|_| "无法计算相对路径".to_string())?;
                Ok(relative_path.to_str().map(|s| s.to_string()))
            } else {
                Ok(None)
            }
        }
    }
}

/// 获取群组头像（带缓存）
/// 返回相对于应用数据目录的相对路径，例如：user_956279803/avatars/group_xxx.png
#[tauri::command]
pub async fn get_group_avatar(
    group_id: i64,
    self_id: Option<i64>,
    app: AppHandle,
) -> Result<Option<String>, String> {
    let app_data_dir = app.path()
        .app_data_dir()
        .map_err(|e| format!("获取应用数据目录失败: {}", e))?;
    
    let cache_dir = get_avatar_cache_dir(&app, self_id)?;
    let filename = get_avatar_filename(group_id, true);
    let cache_path = cache_dir.join(&filename);
    
    // 检查缓存是否存在且未过期（7天）
    if cache_path.exists() {
        if let Ok(metadata) = fs::metadata(&cache_path) {
            if let Ok(modified) = metadata.modified() {
                let age = std::time::SystemTime::now()
                    .duration_since(modified)
                    .unwrap_or_default();
                
                // 如果缓存未过期（7天），返回相对路径
                if age.as_secs() < 7 * 24 * 60 * 60 {
                    let relative_path = cache_path.strip_prefix(&app_data_dir)
                        .map_err(|_| "无法计算相对路径".to_string())?;
                    return Ok(relative_path.to_str().map(|s| s.to_string()));
                }
            }
        }
    }
    
    // 下载头像（群组头像使用群号）
    let url = format!("http://p.qlogo.cn/gh/{}/{}/640", group_id, group_id);
    
    match download_avatar(&url, &cache_path).await {
        Ok(_) => {
            let relative_path = cache_path.strip_prefix(&app_data_dir)
                .map_err(|_| "无法计算相对路径".to_string())?;
            Ok(relative_path.to_str().map(|s| s.to_string()))
        }
        Err(e) => {
            eprintln!("下载头像失败: {}", e);
            // 如果下载失败但缓存存在，返回相对路径
            if cache_path.exists() {
                let relative_path = cache_path.strip_prefix(&app_data_dir)
                    .map_err(|_| "无法计算相对路径".to_string())?;
                Ok(relative_path.to_str().map(|s| s.to_string()))
            } else {
                Ok(None)
            }
        }
    }
}

/// 清除头像缓存
#[tauri::command]
pub async fn clear_avatar_cache(
    self_id: Option<i64>,
    app: AppHandle,
) -> Result<(), String> {
    let cache_dir = get_avatar_cache_dir(&app, self_id)?;
    
    if cache_dir.exists() {
        fs::remove_dir_all(&cache_dir)
            .map_err(|e| format!("清除头像缓存失败: {}", e))?;
        
        // 重新创建目录
        fs::create_dir_all(&cache_dir)
            .map_err(|e| format!("重新创建头像缓存目录失败: {}", e))?;
    }
    
    Ok(())
}

/// 下载头像并保存到缓存（同步版本，用于协议处理器）
fn download_avatar_sync(url: &str, cache_path: &Path) -> Result<(), String> {
    // 使用 reqwest 的阻塞客户端
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(|e| format!("创建 HTTP 客户端失败: {}", e))?;
    
    let response = client
        .get(url)
        .send()
        .map_err(|e| format!("下载头像失败: {}", e))?;
    
    if !response.status().is_success() {
        return Err(format!("下载头像失败: HTTP {}", response.status()));
    }
    
    let bytes = response
        .bytes()
        .map_err(|e| format!("读取头像数据失败: {}", e))?;
    
    // 确保父目录存在
    if let Some(parent) = cache_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("创建头像缓存目录失败: {}", e))?;
    }
    
    // 保存到文件
    let mut file = fs::File::create(cache_path)
        .map_err(|e| format!("创建头像文件失败: {}", e))?;
    
    file.write_all(&bytes)
        .map_err(|e| format!("写入头像文件失败: {}", e))?;
    
    Ok(())
}

/// 获取用户或群组头像（同步版本，用于协议处理器）
pub fn get_user_avatar_sync(
    id: i64,
    self_id: Option<i64>,
    app_data_dir: &str,
    is_group: bool,
) -> Result<Option<String>, String> {
    let app_data_dir_path = std::path::Path::new(app_data_dir);
    
    // 构建缓存目录路径
    let mut cache_dir = app_data_dir_path.to_path_buf();
    if let Some(uid) = self_id {
        cache_dir.push(format!("user_{}", uid));
    }
    cache_dir.push("avatars");
    
    // 确保目录存在
    fs::create_dir_all(&cache_dir)
        .map_err(|e| format!("创建头像缓存目录失败: {}", e))?;
    
    let filename = get_avatar_filename(id, is_group);
    let cache_path = cache_dir.join(&filename);
    
    // 检查缓存是否存在且未过期（7天）
    if cache_path.exists() {
        if let Ok(metadata) = fs::metadata(&cache_path) {
            if let Ok(modified) = metadata.modified() {
                let age = std::time::SystemTime::now()
                    .duration_since(modified)
                    .unwrap_or_default();
                
                // 如果缓存未过期（7天），返回相对路径
                if age.as_secs() < 7 * 24 * 60 * 60 {
                    let relative_path = cache_path.strip_prefix(app_data_dir_path)
                        .map_err(|_| "无法计算相对路径".to_string())?;
                    return Ok(relative_path.to_str().map(|s| s.to_string()));
                }
            }
        }
    }
    
    // 下载头像
    let url = if is_group {
        format!("http://p.qlogo.cn/gh/{}/{}/640", id, id)
    } else {
        format!("http://q.qlogo.cn/headimg_dl?dst_uin={}&spec=640&img_type=png", id)
    };
    
    match download_avatar_sync(&url, &cache_path) {
        Ok(_) => {
            let relative_path = cache_path.strip_prefix(app_data_dir_path)
                .map_err(|_| "无法计算相对路径".to_string())?;
            Ok(relative_path.to_str().map(|s| s.to_string()))
        }
        Err(e) => {
            eprintln!("下载头像失败: {}", e);
            // 如果下载失败但缓存存在，返回相对路径
            if cache_path.exists() {
                let relative_path = cache_path.strip_prefix(app_data_dir_path)
                    .map_err(|_| "无法计算相对路径".to_string())?;
                Ok(relative_path.to_str().map(|s| s.to_string()))
            } else {
                Ok(None)
            }
        }
    }
}

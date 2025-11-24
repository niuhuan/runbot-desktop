use tauri::{AppHandle, Manager};
use rusqlite::{Connection, Result as SqlResult, params};
use serde_json::Value;
use std::path::PathBuf;
use sha2::{Sha256, Digest};
use base64::{Engine as _, engine::general_purpose};

/// 获取应用数据目录路径，并确保目录存在
fn ensure_app_data_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app.path()
        .app_data_dir()
        .map_err(|e| format!("获取应用数据目录失败: {}", e))?;
    
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("创建应用数据目录失败: {}", e))?;
    
    Ok(dir)
}

/// 获取用户数据目录（根据 self_id）
fn get_user_data_dir(app: &AppHandle, self_id: Option<i64>) -> Result<PathBuf, String> {
    let mut path = ensure_app_data_dir(app)?;
    
    if let Some(uid) = self_id {
        path.push(format!("user_{}", uid));
        std::fs::create_dir_all(&path)
            .map_err(|e| format!("创建用户数据目录失败: {}", e))?;
    }
    
    Ok(path)
}

/// 获取数据库路径（用户特定）
fn get_db_path(app: &AppHandle, self_id: Option<i64>) -> Result<PathBuf, String> {
    let mut path = get_user_data_dir(app, self_id)?;
    path.push("runbot.db");
    Ok(path)
}

/// 初始化数据库（创建表和索引）
fn init_database(conn: &Connection) -> SqlResult<()> {
    // 创建消息表（使用 localMessageId 作为主键）
    conn.execute(
        "CREATE TABLE IF NOT EXISTS messages (
            local_message_id TEXT PRIMARY KEY,
            timestamp INTEGER NOT NULL,
            post_type TEXT NOT NULL,
            message_type TEXT,
            user_id INTEGER,
            group_id INTEGER,
            message_id INTEGER,
            content TEXT,
            raw_message TEXT,
            data TEXT NOT NULL,
            recalled INTEGER DEFAULT 0,
            created_at INTEGER DEFAULT (strftime('%s', 'now'))
        )",
        [],
    )?;

    // 为已存在的表添加 recalled 字段（如果还没有）
    // SQLite 不支持 "IF NOT EXISTS" 在 ALTER TABLE 中，需要检查
    let column_exists: bool = conn.query_row(
        "SELECT COUNT(*) FROM pragma_table_info('messages') WHERE name='recalled'",
        [],
        |row| row.get(0),
    ).unwrap_or(0) > 0;
    
    if !column_exists {
        conn.execute(
            "ALTER TABLE messages ADD COLUMN recalled INTEGER DEFAULT 0",
            [],
        )?;
    }

    // 创建全文搜索虚拟表（FTS5）
    // 注意：FTS5 需要 rowid，但我们使用 local_message_id 作为主键
    // 所以需要创建一个映射表或者使用 WITHOUT ROWID 的替代方案
    // 这里我们创建一个辅助表来映射 local_message_id 到 rowid
    conn.execute(
        "CREATE TABLE IF NOT EXISTS messages_rowid_map (
            rowid INTEGER PRIMARY KEY AUTOINCREMENT,
            local_message_id TEXT UNIQUE NOT NULL
        )",
        [],
    )?;
    
    conn.execute(
        "CREATE VIRTUAL TABLE IF NOT EXISTS messages_fts USING fts5(
            content,
            raw_message,
            user_id UNINDEXED,
            group_id UNINDEXED,
            post_type UNINDEXED,
            content='messages_rowid_map',
            content_rowid='rowid'
        )",
        [],
    )?;

    // 创建索引
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_messages_timestamp ON messages(timestamp DESC)",
        [],
    )?;
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_messages_user_id ON messages(user_id)",
        [],
    )?;
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_messages_group_id ON messages(group_id)",
        [],
    )?;
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_messages_post_type ON messages(post_type)",
        [],
    )?;

    // 创建请求表
    // 注意: 使用 user_id 和 group_id 组合来确保唯一性
    // 好友请求: user_id 唯一 (group_id 为 NULL)
    // 群请求: (group_id, user_id) 组合唯一
    conn.execute(
        "CREATE TABLE IF NOT EXISTS requests (
            id TEXT PRIMARY KEY,
            timestamp INTEGER NOT NULL,
            request_type TEXT NOT NULL,
            sub_type TEXT,
            user_id INTEGER NOT NULL,
            user_name TEXT NOT NULL,
            nickname TEXT,
            comment TEXT NOT NULL,
            flag TEXT NOT NULL,
            group_id INTEGER,
            group_name TEXT,
            status TEXT NOT NULL DEFAULT 'pending',
            is_read INTEGER NOT NULL DEFAULT 0,
            created_at INTEGER DEFAULT (strftime('%s', 'now')),
            UNIQUE(user_id, group_id)
        )",
        [],
    )?;

    // 检查并添加 is_read 字段（如果表已存在但没有该字段）
    let is_read_exists: bool = conn.query_row(
        "SELECT COUNT(*) FROM pragma_table_info('requests') WHERE name='is_read'",
        [],
        |row| row.get(0),
    ).unwrap_or(0) > 0;
    
    if !is_read_exists {
        conn.execute(
            "ALTER TABLE requests ADD COLUMN is_read INTEGER NOT NULL DEFAULT 0",
            [],
        )?;
    }

    // 创建请求表索引
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_requests_timestamp ON requests(timestamp DESC)",
        [],
    )?;
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_requests_status ON requests(status)",
        [],
    )?;
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_requests_user_id ON requests(user_id)",
        [],
    )?;
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_requests_flag ON requests(flag)",
        [],
    )?;
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_requests_is_read ON requests(is_read)",
        [],
    )?;

    Ok(())
}

/// 获取数据库连接（用户特定）
fn get_connection(app: &AppHandle, self_id: Option<i64>) -> Result<Connection, String> {
    let db_path = get_db_path(app, self_id)?;
    
    tracing::info!("🔌 get_connection: self_id={:?}, db_path={:?}", self_id, db_path);
    
    let conn = Connection::open(&db_path)
        .map_err(|e| format!("打开数据库失败: {}", e))?;
    
    // 初始化数据库（如果还没有初始化）
    init_database(&conn)
        .map_err(|e| format!("初始化数据库失败: {}", e))?;
    
    Ok(conn)
}

/// 获取存储文件路径（用于配置存储，用户特定）
fn get_storage_path(app: &AppHandle, key: &str, self_id: Option<i64>) -> Result<PathBuf, String> {
    let mut path = get_user_data_dir(app, self_id)?;
    
    // 使用 key 的 SHA256 哈希作为文件名
    let mut hasher = Sha256::new();
    hasher.update(key.as_bytes());
    let hash = hasher.finalize();
    let filename = general_purpose::STANDARD.encode(hash);
    path.push(format!("{}.json", filename));
    
    Ok(path)
}

// ========== 配置存储（JSON 文件） ==========

/// 保存配置（用户特定）
#[tauri::command]
pub async fn save_config(
    key: String,
    value: String,
    self_id: Option<i64>,
    app: AppHandle,
) -> Result<(), String> {
    let path = get_storage_path(&app, &key, self_id)?;
    
    // 确保父目录存在
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("创建目录失败: {}", e))?;
    }
    
    tokio::fs::write(&path, value)
        .await
        .map_err(|e| format!("写入文件失败: {}", e))?;
    
    Ok(())
}

/// 读取配置（用户特定）
#[tauri::command]
pub async fn load_config(
    key: String,
    self_id: Option<i64>,
    app: AppHandle,
) -> Result<Option<String>, String> {
    let path = get_storage_path(&app, &key, self_id)?;
    
    if !path.exists() {
        return Ok(None);
    }
    
    match tokio::fs::read_to_string(&path).await {
        Ok(content) => Ok(Some(content)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("读取文件失败: {}", e)),
    }
}

/// 删除配置（用户特定）
#[tauri::command]
pub async fn remove_config(
    key: String,
    self_id: Option<i64>,
    app: AppHandle,
) -> Result<(), String> {
    let path = get_storage_path(&app, &key, self_id)?;
    
    if path.exists() {
        tokio::fs::remove_file(&path)
            .await
            .map_err(|e| format!("删除文件失败: {}", e))?;
    }
    
    Ok(())
}

// ========== 消息存储（rusqlite + FTS5） ==========

/// 保存消息（用户特定）
#[tauri::command]
pub async fn save_message(
    message_data: String,
    self_id: Option<i64>,
    app: AppHandle,
) -> Result<String, String> {
    let conn = get_connection(&app, self_id)?;
    
    // 解析 JSON 数据
    let msg: Value = serde_json::from_str(&message_data)
        .map_err(|e| format!("解析 JSON 失败: {}", e))?;
    
    // 获取或生成 localMessageId
    let local_message_id = msg["localMessageId"]
        .as_str()
        .ok_or_else(|| "缺少 localMessageId 字段".to_string())?
        .to_string();
    
    let timestamp = msg["time"].as_i64()
        .ok_or_else(|| "缺少 time 字段".to_string())?;
    let post_type = msg["post_type"].as_str()
        .ok_or_else(|| "缺少 post_type 字段".to_string())?
        .to_string();
    let message_type = msg["message_type"].as_str().map(|s| s.to_string());
    let user_id = msg["user_id"].as_i64();
    let group_id = msg["group_id"].as_i64();
    let message_id = msg["message_id"].as_i64();
    let content = msg["message"].as_str().map(|s| s.to_string());
    let raw_message = msg["raw_message"].as_str().map(|s| s.to_string());
    
    // 插入或更新消息（使用 INSERT OR REPLACE）
    conn.execute(
        "INSERT OR REPLACE INTO messages (
            local_message_id, timestamp, post_type, message_type, user_id, group_id,
            message_id, content, raw_message, data
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        params![
            local_message_id,
            timestamp,
            post_type,
            message_type,
            user_id,
            group_id,
            message_id,
            content,
            raw_message,
            message_data
        ],
    )
    .map_err(|e| format!("插入消息失败: {}", e))?;
    
    // 获取或创建 rowid 映射
    let row_id = match conn.query_row::<i64, _, _>(
        "SELECT rowid FROM messages_rowid_map WHERE local_message_id = ?1",
        params![local_message_id],
        |row| row.get(0),
    ) {
        Ok(id) => id,
        Err(_) => {
            // 如果不存在，插入新的映射
            conn.execute(
                "INSERT INTO messages_rowid_map (local_message_id) VALUES (?1)",
                params![local_message_id],
            )
            .map_err(|e| format!("插入 rowid 映射失败: {}", e))?;
            conn.last_insert_rowid()
        }
    };
    
    // 更新全文搜索索引（FTS5 会自动同步，但我们可以手动插入以确保一致性）
    let content_str = content.as_deref().unwrap_or("");
    let raw_msg_str = raw_message.as_deref().unwrap_or("");
    
    // 先删除旧的索引（如果存在）
    conn.execute(
        "DELETE FROM messages_fts WHERE rowid = ?1",
        params![row_id],
    )
    .ok(); // 忽略错误，可能不存在
    
    // 插入新的索引
    conn.execute(
        "INSERT INTO messages_fts (rowid, content, raw_message, user_id, group_id, post_type)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            row_id,
            content_str,
            raw_msg_str,
            user_id.unwrap_or(0),
            group_id.unwrap_or(0),
            post_type
        ],
    )
    .map_err(|e| format!("更新全文搜索索引失败: {}", e))?;
    
    Ok(local_message_id)
}

/// 更新消息的 message_id（用户特定）
#[tauri::command]
pub async fn update_message_id(
    local_message_id: String,
    message_id: i64,
    self_id: Option<i64>,
    app: AppHandle,
) -> Result<(), String> {
    let conn = get_connection(&app, self_id)?;
    
    // 更新消息的 message_id
    conn.execute(
        "UPDATE messages SET message_id = ?1 WHERE local_message_id = ?2",
        params![message_id, local_message_id],
    )
    .map_err(|e| format!("更新消息 message_id 失败: {}", e))?;
    
    // 同时更新 data 字段中的 message_id
    let mut msg_data: Value = conn.query_row(
        "SELECT data FROM messages WHERE local_message_id = ?1",
        params![local_message_id],
        |row| {
            let data_str: String = row.get(0)?;
            Ok(serde_json::from_str::<Value>(&data_str).unwrap_or(Value::Null))
        },
    )
    .map_err(|e| format!("获取消息数据失败: {}", e))?;
    
    if let Some(obj) = msg_data.as_object_mut() {
        obj.insert("message_id".to_string(), Value::Number(message_id.into()));
        let updated_data = serde_json::to_string(&msg_data)
            .map_err(|e| format!("序列化消息数据失败: {}", e))?;
        
        conn.execute(
            "UPDATE messages SET data = ?1 WHERE local_message_id = ?2",
            params![updated_data, local_message_id],
        )
        .map_err(|e| format!("更新消息数据失败: {}", e))?;
    }
    
    Ok(())
}

/// 更新消息内容（用于替换 base64 图片为正常 URL）
#[tauri::command]
pub async fn update_message_content(
    local_message_id: String,
    message: String,
    raw_message: String,
    self_id: Option<i64>,
    app: AppHandle,
) -> Result<(), String> {
    let conn = get_connection(&app, self_id)?;
    
    // 更新消息的 content 和 raw_message 字段
    conn.execute(
        "UPDATE messages SET content = ?1, raw_message = ?2 WHERE local_message_id = ?3",
        params![message, raw_message, local_message_id],
    )
    .map_err(|e| format!("更新消息内容失败: {}", e))?;
    
    // 同时更新 data 字段中的 message 和 raw_message
    let mut msg_data: Value = conn.query_row(
        "SELECT data FROM messages WHERE local_message_id = ?1",
        params![local_message_id],
        |row| {
            let data_str: String = row.get(0)?;
            Ok(serde_json::from_str::<Value>(&data_str).unwrap_or(Value::Null))
        },
    )
    .map_err(|e| format!("获取消息数据失败: {}", e))?;
    
    if let Some(obj) = msg_data.as_object_mut() {
        obj.insert("message".to_string(), Value::String(message.clone()));
        obj.insert("raw_message".to_string(), Value::String(raw_message.clone()));
        let updated_data = serde_json::to_string(&msg_data)
            .map_err(|e| format!("序列化消息数据失败: {}", e))?;
        
        conn.execute(
            "UPDATE messages SET data = ?1 WHERE local_message_id = ?2",
            params![updated_data, local_message_id],
        )
        .map_err(|e| format!("更新消息数据失败: {}", e))?;
    }
    
    Ok(())
}

/// 获取消息列表（用户特定）
#[tauri::command]
pub async fn get_messages(
    limit: Option<u32>,
    offset: Option<u32>,
    post_type: Option<String>,
    user_id: Option<i64>,
    group_id: Option<i64>,
    self_id: Option<i64>,
    app: AppHandle,
) -> Result<Vec<String>, String> {
    let conn = get_connection(&app, self_id)?;
    let limit = limit.unwrap_or(100);
    let offset = offset.unwrap_or(0);
    
    // 修改查询以同时获取 data 和 recalled 字段
    let mut query = "SELECT data, recalled FROM messages WHERE 1=1".to_string();
    let mut params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
    
    if let Some(pt) = &post_type {
        query.push_str(" AND post_type = ?");
        params.push(Box::new(pt.clone()));
    }
    
    if let Some(uid) = user_id {
        query.push_str(" AND user_id = ?");
        params.push(Box::new(uid));
    }
    
    if let Some(gid) = group_id {
        query.push_str(" AND group_id = ?");
        params.push(Box::new(gid));
    }
    
    query.push_str(" ORDER BY timestamp DESC LIMIT ? OFFSET ?");
    params.push(Box::new(limit as i32));
    params.push(Box::new(offset as i32));
    
    // 使用统一的查询逻辑
    let mut stmt = conn.prepare(&query)
        .map_err(|e| format!("准备查询失败: {}", e))?;
    
    // 构建参数引用数组
    let param_refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|p| p.as_ref()).collect();
    
    // 使用统一的闭包处理所有情况
    let rows = stmt.query_map(
        rusqlite::params_from_iter(param_refs.iter().copied()),
        |row| -> SqlResult<String> {
            let data: String = row.get(0)?;
            let recalled: i64 = row.get(1)?;
            
            // 解析 JSON 并添加 recalled 字段
            if let Ok(mut json_value) = serde_json::from_str::<Value>(&data) {
                if let Some(obj) = json_value.as_object_mut() {
                    obj.insert("recalled".to_string(), Value::Bool(recalled != 0));
                }
                Ok(serde_json::to_string(&json_value).unwrap_or(data))
            } else {
                Ok(data)
            }
        },
    )
    .map_err(|e| format!("执行查询失败: {}", e))?;
    
    let mut messages = Vec::new();
    for row in rows {
        messages.push(row.map_err(|e| format!("读取行失败: {}", e))?);
    }
    
    Ok(messages)
}

/// 搜索消息（全文搜索，用户特定）
#[tauri::command]
pub async fn search_messages(
    query: String,
    limit: Option<u32>,
    offset: Option<u32>,
    self_id: Option<i64>,
    app: AppHandle,
) -> Result<Vec<String>, String> {
    let conn = get_connection(&app, self_id)?;
    let limit = limit.unwrap_or(100);
    let offset = offset.unwrap_or(0);
    
    // 使用 FTS5 全文搜索，同时获取 recalled 字段
    let mut stmt = conn.prepare(
        "SELECT m.data, m.recalled FROM messages m
         JOIN messages_rowid_map rmap ON m.local_message_id = rmap.local_message_id
         JOIN messages_fts fts ON rmap.rowid = fts.rowid
         WHERE messages_fts MATCH ?1
         ORDER BY m.timestamp DESC
         LIMIT ?2 OFFSET ?3"
    )
    .map_err(|e| format!("准备搜索查询失败: {}", e))?;
    
    let rows = stmt.query_map(
        params![query, limit as i32, offset as i32],
        |row| {
            let data: String = row.get(0)?;
            let recalled: i64 = row.get(1)?;
            
            // 解析 JSON 并添加 recalled 字段
            if let Ok(mut json_value) = serde_json::from_str::<Value>(&data) {
                if let Some(obj) = json_value.as_object_mut() {
                    obj.insert("recalled".to_string(), Value::Bool(recalled != 0));
                }
                Ok(serde_json::to_string(&json_value).unwrap_or(data))
            } else {
                Ok(data)
            }
        },
    )
    .map_err(|e| format!("执行搜索失败: {}", e))?;
    
    let mut messages = Vec::new();
    for row in rows {
        messages.push(row.map_err(|e| format!("读取行失败: {}", e))?);
    }
    
    Ok(messages)
}

/// 删除消息（用户特定）
#[tauri::command]
pub async fn delete_message(
    local_message_id: String,
    self_id: Option<i64>,
    app: AppHandle,
) -> Result<(), String> {
    let conn = get_connection(&app, self_id)?;
    
    // 获取 rowid
    let row_id: Option<i64> = conn.query_row(
        "SELECT rowid FROM messages_rowid_map WHERE local_message_id = ?1",
        params![local_message_id],
        |row| row.get(0),
    ).ok();
    
    if let Some(rid) = row_id {
        // 删除全文搜索索引
        conn.execute(
            "DELETE FROM messages_fts WHERE rowid = ?1",
            params![rid],
        )
        .map_err(|e| format!("删除全文搜索索引失败: {}", e))?;
        
        // 删除 rowid 映射
        conn.execute(
            "DELETE FROM messages_rowid_map WHERE rowid = ?1",
            params![rid],
        )
        .map_err(|e| format!("删除 rowid 映射失败: {}", e))?;
    }
    
    // 删除消息
    conn.execute(
        "DELETE FROM messages WHERE local_message_id = ?1",
        params![local_message_id],
    )
    .map_err(|e| format!("删除消息失败: {}", e))?;
    
    Ok(())
}

/// 清理旧消息（保留最近 N 条，用户特定）
#[tauri::command]
pub async fn cleanup_old_messages(
    keep_count: u32,
    self_id: Option<i64>,
    app: AppHandle,
) -> Result<u32, String> {
    let conn = get_connection(&app, self_id)?;
    
    // 获取需要删除的消息 ID
    let mut stmt = conn.prepare(
        "SELECT id FROM messages ORDER BY timestamp DESC LIMIT -1 OFFSET ?1"
    )
    .map_err(|e| format!("准备清理查询失败: {}", e))?;
    
    let rows = stmt.query_map(
        params![keep_count as i32],
        |row| {
            let id: i64 = row.get(0)?;
            Ok(id)
        },
    )
    .map_err(|e| format!("执行清理查询失败: {}", e))?;
    
    let mut deleted_count = 0u32;
    for row in rows {
        let id = row.map_err(|e| format!("读取行失败: {}", e))?;
        
        // 删除全文搜索索引
        conn.execute(
            "DELETE FROM messages_fts WHERE rowid = ?1",
            params![id],
        )
        .ok();
        
        // 删除消息
        conn.execute(
            "DELETE FROM messages WHERE id = ?1",
            params![id],
        )
        .ok();
        
        deleted_count += 1;
    }
    
    Ok(deleted_count)
}

/// 获取消息统计信息（用户特定）
#[tauri::command]
pub async fn get_message_stats(
    self_id: Option<i64>,
    app: AppHandle,
) -> Result<serde_json::Value, String> {
    let conn = get_connection(&app, self_id)?;
    
    let total_count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM messages",
        [],
        |row| row.get(0),
    )
    .map_err(|e| format!("获取消息总数失败: {}", e))?;
    
    let message_count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM messages WHERE post_type = 'message'",
        [],
        |row| row.get(0),
    )
    .unwrap_or(0);
    
    let notice_count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM messages WHERE post_type = 'notice'",
        [],
        |row| row.get(0),
    )
    .unwrap_or(0);
    
    Ok(serde_json::json!({
        "total": total_count,
        "messages": message_count,
        "notices": notice_count,
    }))
}

/// 标记消息为已撤回（通过 message_id）
#[tauri::command]
pub async fn mark_message_recalled(
    message_id: i64,
    self_id: Option<i64>,
    app: AppHandle,
) -> Result<(), String> {
    let conn = get_connection(&app, self_id)?;
    
    // 通过 message_id 标记消息为已撤回
    let affected = conn.execute(
        "UPDATE messages SET recalled = 1 WHERE message_id = ?1",
        params![message_id],
    )
    .map_err(|e| format!("标记消息为已撤回失败: {}", e))?;
    
    if affected > 0 {
        tracing::info!("已标记 message_id={} 的消息为已撤回", message_id);
    } else {
        tracing::warn!("未找到 message_id={} 的消息", message_id);
    }
    
    Ok(())
}

/// 查询消息是否已撤回（调试用）
#[tauri::command]
pub async fn check_message_recalled(
    message_id: i64,
    self_id: Option<i64>,
    app: AppHandle,
) -> Result<serde_json::Value, String> {
    let conn = get_connection(&app, self_id)?;
    
    let result = conn.query_row(
        "SELECT message_id, recalled, raw_message, timestamp FROM messages WHERE message_id = ?1",
        params![message_id],
        |row| {
            Ok(serde_json::json!({
                "message_id": row.get::<_, i64>(0)?,
                "recalled": row.get::<_, i64>(1)?,
                "raw_message": row.get::<_, Option<String>>(2)?,
                "timestamp": row.get::<_, i64>(3)?,
            }))
        },
    );
    
    match result {
        Ok(data) => Ok(data),
        Err(rusqlite::Error::QueryReturnedNoRows) => {
            Ok(serde_json::json!({
                "error": "未找到该消息",
                "message_id": message_id
            }))
        }
        Err(e) => Err(format!("查询消息失败: {}", e))
    }
}

// ========== 请求存储 ==========

/// 保存请求
#[tauri::command]
pub async fn save_request(
    request_data: String,
    self_id: Option<i64>,
    app: AppHandle,
) -> Result<String, String> {
    tracing::info!("📝 save_request 被调用: self_id={:?}", self_id);
    
    let conn = get_connection(&app, self_id)?;
    
    tracing::info!("✅ 获取数据库连接成功");
    
    // 解析 JSON 数据
    let req: Value = serde_json::from_str(&request_data)
        .map_err(|e| format!("解析 JSON 失败: {}", e))?;
    
    let id = req["id"].as_str()
        .ok_or_else(|| "缺少 id 字段".to_string())?
        .to_string();
    let timestamp = req["time"].as_i64()
        .ok_or_else(|| "缺少 time 字段".to_string())?;
    let request_type = req["request_type"].as_str()
        .ok_or_else(|| "缺少 request_type 字段".to_string())?
        .to_string();
    let sub_type = req["sub_type"].as_str().map(|s| s.to_string());
    let user_id = req["user_id"].as_i64()
        .ok_or_else(|| "缺少 user_id 字段".to_string())?;
    let user_name = req["user_name"].as_str()
        .ok_or_else(|| "缺少 user_name 字段".to_string())?
        .to_string();
    let nickname = req["nickname"].as_str().map(|s| s.to_string());
    let comment = req["comment"].as_str()
        .unwrap_or("")
        .to_string();
    let flag = req["flag"].as_str()
        .ok_or_else(|| "缺少 flag 字段".to_string())?
        .to_string();
    let group_id = req["group_id"].as_i64();
    let group_name = req["group_name"].as_str().map(|s| s.to_string());
    let status = req["status"].as_str()
        .unwrap_or("pending")
        .to_string();
    let is_read = req["is_read"].as_bool().unwrap_or(false);
    
    tracing::info!("📊 解析请求数据: id={}, flag={}, request_type={}, user_id={}, group_id={:?}, status={}, is_read={}", 
        id, flag, request_type, user_id, group_id, status, is_read);
    
    // 插入或替换请求（基于 user_id 和 group_id 的唯一约束）
    // 当收到同一个用户的新请求时，会自动更新旧记录
    // 好友请求: 同一个 user_id (group_id=NULL) 只保留最新的
    // 群请求: 同一个 (group_id, user_id) 组合只保留最新的
    let result = conn.execute(
        "INSERT OR REPLACE INTO requests (
            id, timestamp, request_type, sub_type, user_id, user_name, nickname,
            comment, flag, group_id, group_name, status, is_read
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
        params![
            id,
            timestamp,
            request_type,
            sub_type,
            user_id,
            user_name,
            nickname,
            comment,
            flag,
            group_id,
            group_name,
            status,
            is_read as i64
        ],
    )
    .map_err(|e| format!("插入请求失败: {}", e))?;
    
    tracing::info!("✅ 成功保存/更新请求到数据库: id={}, user_id={}, group_id={:?}, 影响行数={}", 
        id, user_id, group_id, result);
    
    Ok(id)
}

/// 更新请求状态
#[tauri::command]
pub async fn update_request_status(
    flag: String,
    status: String,
    self_id: Option<i64>,
    app: AppHandle,
) -> Result<(), String> {
    let conn = get_connection(&app, self_id)?;
    
    conn.execute(
        "UPDATE requests SET status = ?1 WHERE flag = ?2",
        params![status, flag],
    )
    .map_err(|e| format!("更新请求状态失败: {}", e))?;
    
    Ok(())
}

/// 获取请求列表
#[tauri::command]
pub async fn get_requests(
    status: Option<String>,
    limit: Option<u32>,
    offset: Option<u32>,
    self_id: Option<i64>,
    app: AppHandle,
) -> Result<Vec<String>, String> {
    tracing::info!("🔍 get_requests 被调用: self_id={:?}, status={:?}, limit={:?}, offset={:?}", 
        self_id, status, limit, offset);
    
    let conn = get_connection(&app, self_id)?;
    
    tracing::info!("✅ 获取数据库连接成功");
    
    let limit = limit.unwrap_or(100);
    let offset = offset.unwrap_or(0);
    
    let mut query = "SELECT id, timestamp, request_type, sub_type, user_id, user_name, nickname, \
                     comment, flag, group_id, group_name, status, is_read FROM requests WHERE 1=1".to_string();
    let mut params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
    
    if let Some(s) = &status {
        query.push_str(" AND status = ?");
        params.push(Box::new(s.clone()));
    }
    
    query.push_str(" ORDER BY timestamp DESC LIMIT ? OFFSET ?");
    params.push(Box::new(limit as i32));
    params.push(Box::new(offset as i32));
    
    tracing::info!("📝 执行查询 SQL: {}", query);
    
    let mut stmt = conn.prepare(&query)
        .map_err(|e| format!("准备查询失败: {}", e))?;
    
    let param_refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|p| p.as_ref()).collect();
    
    let rows = stmt.query_map(
        rusqlite::params_from_iter(param_refs.iter().copied()),
        |row| {
            let id: String = row.get(0)?;
            let timestamp: i64 = row.get(1)?;
            let request_type: String = row.get(2)?;
            let sub_type: Option<String> = row.get(3)?;
            let user_id: i64 = row.get(4)?;
            let user_name: String = row.get(5)?;
            let nickname: Option<String> = row.get(6)?;
            let comment: String = row.get(7)?;
            let flag: String = row.get(8)?;
            let group_id: Option<i64> = row.get(9)?;
            let group_name: Option<String> = row.get(10)?;
            let status: String = row.get(11)?;
            let is_read: i64 = row.get(12)?;
            
            let json = serde_json::json!({
                "id": id,
                "time": timestamp,
                "request_type": request_type,
                "sub_type": sub_type,
                "user_id": user_id,
                "user_name": user_name,
                "nickname": nickname,
                "comment": comment,
                "flag": flag,
                "group_id": group_id,
                "group_name": group_name,
                "status": status,
                "is_read": is_read != 0
            });
            
            Ok(serde_json::to_string(&json).unwrap_or_default())
        },
    )
    .map_err(|e| format!("执行查询失败: {}", e))?;
    
    let mut requests = Vec::new();
    for row in rows {
        requests.push(row.map_err(|e| format!("读取行失败: {}", e))?);
    }
    
    tracing::info!("✅ 查询完成，找到 {} 个请求", requests.len());
    
    Ok(requests)
}

/// 删除请求
#[tauri::command]
pub async fn delete_request(
    flag: String,
    self_id: Option<i64>,
    app: AppHandle,
) -> Result<(), String> {
    let conn = get_connection(&app, self_id)?;
    
    conn.execute(
        "DELETE FROM requests WHERE flag = ?1",
        params![flag],
    )
    .map_err(|e| format!("删除请求失败: {}", e))?;
    
    Ok(())
}

/// 清空历史请求（只保留待处理的）
#[tauri::command]
pub async fn clear_history_requests(
    self_id: Option<i64>,
    app: AppHandle,
) -> Result<u32, String> {
    let conn = get_connection(&app, self_id)?;
    
    let deleted = conn.execute(
        "DELETE FROM requests WHERE status != 'pending'",
        [],
    )
    .map_err(|e| format!("清空历史请求失败: {}", e))?;
    
    Ok(deleted as u32)
}

/// 标记请求为已读
#[tauri::command]
pub async fn mark_request_read(
    flag: String,
    self_id: Option<i64>,
    app: AppHandle,
) -> Result<(), String> {
    let conn = get_connection(&app, self_id)?;
    
    conn.execute(
        "UPDATE requests SET is_read = 1 WHERE flag = ?1",
        params![flag],
    )
    .map_err(|e| format!("标记请求为已读失败: {}", e))?;
    
    Ok(())
}

/// 获取未读请求数量
#[tauri::command]
pub async fn get_unread_request_count(
    self_id: Option<i64>,
    app: AppHandle,
) -> Result<i64, String> {
    let conn = get_connection(&app, self_id)?;
    
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM requests WHERE status = 'pending' AND is_read = 0",
        [],
        |row| row.get(0),
    )
    .map_err(|e| format!("获取未读请求数量失败: {}", e))?;
    
    Ok(count)
}

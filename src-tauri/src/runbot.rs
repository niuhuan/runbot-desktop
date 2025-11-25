use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, State};
use serde::{Deserialize, Serialize};
use anyhow::Result;
use runbot::prelude::*;
use async_trait::async_trait;
use tracing;
use crate::CURRENT_SELF_ID;

/// Runbot 客户端状态
#[derive(Debug, Clone, Default)]
pub struct RunbotState {
    pub connected: bool,
    pub ws_url: Option<String>,
    pub access_token: Option<String>,
    pub bot_ctx: Option<Arc<BotContext>>,
    pub self_id: Option<i64>, // 当前登录的 QQ 号
    pub app_handle: Option<AppHandle>, // Tauri AppHandle，用于发送事件
}

/// 连接状态事件
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionStatus {
    pub status: String, // "connected" | "disconnected" | "connecting" | "error"
    pub message: Option<String>,
}

/// 消息事件（OneBot v11 标准格式，用于前端）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OneBotMessage {
    pub time: i64,
    pub self_id: i64,
    pub post_type: String,
    pub message_type: Option<String>,
    pub sub_type: Option<String>,
    pub message_id: Option<i64>,
    pub user_id: Option<i64>,
    pub group_id: Option<i64>,
    pub message: Option<String>,
    pub raw_message: Option<String>,
    pub sender: Option<serde_json::Value>,
    pub raw: Option<serde_json::Value>,
    // 请求相关字段
    pub request_type: Option<String>,  // friend, group
    pub comment: Option<String>,       // 验证消息
    pub flag: Option<String>,          // 请求标识
}

// 将 runbot::event::Message 转换为 CQ 码格式的字符串
#[allow(unused)]
fn message_to_cqcode(message: &runbot::event::Message) -> String {
    let mut result = String::new();
    for msg_data in &message.message {
        match msg_data {
            runbot::event::MessageData::Text(text) => {
                result.push_str(&text.text);
            }
            runbot::event::MessageData::Image(img) => {
                let mut cq_code = String::from("[CQ:image");
                if !img.file.is_empty() {
                    cq_code.push_str(&format!(",file={}", img.file));
                }
                if !img.url.is_empty() {
                    cq_code.push_str(&format!(",url={}", img.url));
                }
                cq_code.push_str(&format!(",sub_type={}", img.sub_type));
                cq_code.push(']');
                result.push_str(&cq_code);
            }
            runbot::event::MessageData::Face(face) => {
                let mut cq_code = String::from("[CQ:face");
                cq_code.push_str(&format!(",id={}", face.id));
                cq_code.push(']');
                result.push_str(&cq_code);
            }
            runbot::event::MessageData::At(at) => {
                let mut cq_code = String::from("[CQ:at");
                cq_code.push_str(&format!(",qq={}", at.qq));
                cq_code.push(']');
                result.push_str(&cq_code);
            }
            _ => {
                // 其他类型的消息数据，暂时忽略或使用原始格式
            }
        }
    }
    result
}

// 将 runbot::event::Post 转换为 OneBotMessage
fn post_to_onebot_message(post: &runbot::event::Post, self_id: i64, app_handle: &tauri::AppHandle) -> Option<OneBotMessage> {
    match post {
        runbot::event::Post::Message(msg) => {
            Some(OneBotMessage {
                time: msg.time,
                self_id,
                post_type: "message".to_string(),
                message_type: Some(match &msg.message_type {
                    runbot::event::MessageType::Private => "private".to_string(),
                    runbot::event::MessageType::Group => "group".to_string(),
                    runbot::event::MessageType::Unknown(s) => s.clone(),
                }),
                sub_type: Some(match &msg.sub_type {
                    runbot::event::MessageSubType::Friend => "friend".to_string(),
                    runbot::event::MessageSubType::Normal => "normal".to_string(),
                    runbot::event::MessageSubType::Unknown(s) => s.clone(),
                }),
                message_id: Some(msg.message_id),
                user_id: Some(msg.user_id),
                group_id: match &msg.message_type {
                    runbot::event::MessageType::Group => Some(msg.group_id),
                    _ => None,
                },
                message: Some(msg.raw_message.clone()),
                raw_message: Some(msg.raw_message.clone()),
                sender: Some(serde_json::json!({
                    "user_id": msg.sender.user_id,
                    "nickname": msg.sender.nickname,
                    "card": msg.sender.card,
                })),
                raw: Some(serde_json::to_value(msg).ok()?),
                request_type: None,
                comment: None,
                flag: None,
            })
        }
        runbot::event::Post::Notice(notice) => {
            // 处理通知事件
            let (notice_type, user_id, group_id) = match notice {
                runbot::event::Notice::GroupUpload(n) => ("group_upload", Some(n.user_id), Some(n.group_id)),
                runbot::event::Notice::GroupAdmin(n) => ("group_admin", Some(n.user_id), Some(n.group_id)),
                runbot::event::Notice::GroupDecrease(n) => ("group_decrease", Some(n.user_id), Some(n.group_id)),
                runbot::event::Notice::GroupIncrease(n) => ("group_increase", Some(n.user_id), Some(n.group_id)),
                runbot::event::Notice::GroupBan(n) => ("group_ban", Some(n.user_id), Some(n.group_id)),
                runbot::event::Notice::FriendAdd(n) => ("friend_add", Some(n.user_id), None),
                runbot::event::Notice::GroupRecall(n) => ("group_recall", Some(n.user_id), Some(n.group_id)),
                runbot::event::Notice::FriendRecall(n) => ("friend_recall", Some(n.user_id), None),
                runbot::event::Notice::Notify(_) => ("notify", None, None),
                runbot::event::Notice::Unknown(_) => ("unknown", None, None),
            };
            
            Some(OneBotMessage {
                time: chrono::Utc::now().timestamp(),
                self_id,
                post_type: "notice".to_string(),
                message_type: None,
                sub_type: Some(notice_type.to_string()),
                message_id: None,
                user_id,
                group_id,
                message: None,
                raw_message: None,
                sender: None,
                raw: Some(serde_json::to_value(notice).ok()?),
                request_type: None,
                comment: None,
                flag: None,
            })
        }
        runbot::event::Post::Request(request) => {
            // 处理请求事件（好友请求、群组请求）
            // 从请求事件中提取 self_id
            let request_self_id = match &request {
                runbot::event::Request::Friend(req) => req.self_id,
                runbot::event::Request::Group(req) => req.self_id,
                _ => self_id,
            };
            
            let (request_type, sub_type, user_id, group_id, comment, flag) = match &request {
                runbot::event::Request::Friend(req) => {
                    ("friend", None, Some(req.user_id), None, Some(req.comment.clone()), Some(req.flag.clone()))
                }
                runbot::event::Request::Group(req) => {
                    let sub_type_str = match &req.sub_type {
                        runbot::event::GroupRequestSubType::Add => "add",
                        runbot::event::GroupRequestSubType::Invite => "invite",
                        runbot::event::GroupRequestSubType::Unknown(s) => s.as_str(),
                    };
                    ("group", Some(sub_type_str.to_string()), Some(req.user_id), Some(req.group_id), Some(req.comment.clone()), Some(req.flag.clone()))
                }
                runbot::event::Request::Unknown(_) => ("unknown", None, None, None, None, None),
            };
            
            // 如果有 flag,先保存到数据库
            if let Some(flag_value) = &flag {
                let flag_str = flag_value.clone();
                let timestamp = chrono::Utc::now().timestamp();
                let request_data = serde_json::json!({
                    "id": format!("{}_{}_{}",  request_type, flag_str, timestamp),
                    "time": timestamp,
                    "request_type": request_type,
                    "sub_type": sub_type,
                    "user_id": user_id.unwrap_or(0),
                    "user_name": format!("用户 {}", user_id.unwrap_or(0)),
                    "nickname": format!("用户 {}", user_id.unwrap_or(0)),
                    "comment": comment.as_deref().unwrap_or(""),
                    "flag": flag_str,
                    "group_id": group_id,
                    "group_name": group_id.map(|id| format!("群 {}", id)),
                    "status": "pending",
                    "is_read": false
                });
                
                tracing::info!("准备保存请求: flag={}, request_self_id={}, request_type={}", 
                    flag_str, request_self_id, request_type);
                
                // 保存到数据库（异步，不阻塞）
                let app_handle_clone = app_handle.clone();
                let request_data_str = serde_json::to_string(&request_data).unwrap_or_default();
                let flag_str_log = flag_str.clone();
                let save_self_id = request_self_id;
                tokio::spawn(async move {
                    tracing::info!("开始保存请求到数据库: flag={}, self_id={}", flag_str_log, save_self_id);
                    match crate::storage::save_request(
                        request_data_str.clone(),
                        Some(save_self_id),
                        app_handle_clone
                    ).await {
                        Ok(id) => {
                            tracing::info!("✅ 成功保存请求到数据库: flag={}, id={}, self_id={}", 
                                flag_str_log, id, save_self_id);
                        }
                        Err(e) => {
                            tracing::error!("❌ 保存请求到数据库失败: flag={}, self_id={}, error={}", 
                                flag_str_log, save_self_id, e);
                        }
                    }
                });
            }
            
            Some(OneBotMessage {
                time: chrono::Utc::now().timestamp(),
                self_id: request_self_id,
                post_type: "request".to_string(),
                message_type: None,
                sub_type,
                message_id: None,
                user_id,
                group_id,
                message: None,
                raw_message: None,
                sender: None,
                raw: Some(serde_json::to_value(request).ok()?),
                request_type: Some(request_type.to_string()),
                comment,
                flag,
            })
        }
        runbot::event::Post::Response(response) => {
            // API 响应
            // runbot 库使用 UUID 作为 echo，我们无法从中提取 action
            // 但我们可以通过检查 data 的内容来推断 action
            // 例如：get_friend_list 返回的是好友数组，get_group_list 返回的是群组数组
            let mut raw_value = serde_json::json!({
                "status": response.status,
                "retcode": response.retcode,
                "data": response.data,
                "message": response.message,
                "echo": response.echo,
            });
            
            // 尝试从 data 的内容推断 action
            let inferred_action = if let Some(data_array) = response.data.as_array() {
                if !data_array.is_empty() {
                    // 检查第一个元素的结构来推断
                    if let Some(first) = data_array.first() {
                        // 群成员列表：有 group_id, user_id, nickname, role 等字段
                        if first.get("group_id").is_some() && first.get("user_id").is_some() && first.get("role").is_some() {
                            Some("get_group_member_list")
                        }
                        // 群组列表：有 group_id, group_name，但没有 user_id
                        else if first.get("group_id").is_some() && first.get("group_name").is_some() && first.get("user_id").is_none() {
                            Some("get_group_list")
                        }
                        // 好友列表：有 user_id, nickname，但没有 group_id
                        else if first.get("user_id").is_some() && first.get("nickname").is_some() && first.get("group_id").is_none() {
                            Some("get_friend_list")
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                } else {
                    None
                }
            } else {
                None
            };
            
            // 调试：记录推断的 action
            tracing::debug!("[Response] inferred_action = {:?}, data_array_len = {}", 
                inferred_action, 
                response.data.as_array().map(|a| a.len()).unwrap_or(0)
            );
            if let Some(data_array) = response.data.as_array() {
                if let Some(first) = data_array.first() {
                    tracing::debug!("[Response] first item keys: {:?}", 
                        first.as_object().map(|o| o.keys().collect::<Vec<_>>())
                    );
                }
            }
            
            // 如果推断到了 action，添加到 raw 中
            if let Some(action) = inferred_action {
                tracing::debug!("[Response] 添加 action 到 raw: {}", action);
                raw_value["action"] = serde_json::Value::String(action.to_string());
            }
            
            Some(OneBotMessage {
                time: chrono::Utc::now().timestamp(),
                self_id,
                post_type: "api_response".to_string(),
                message_type: None,
                sub_type: None,
                message_id: None,
                user_id: None,
                group_id: None,
                message: None,
                raw_message: None,
                sender: None,
                raw: Some(raw_value),
                request_type: None,
                comment: None,
                flag: None,
            })
        }
        runbot::event::Post::MetaEvent(meta) => {
            match meta {
                runbot::event::MetaEvent::Lifecycle(lifecycle) => {
                    Some(OneBotMessage {
                        time: lifecycle.time,
                        self_id: lifecycle.self_id,
                        post_type: "meta_event".to_string(),
                        message_type: None,
                        sub_type: Some("lifecycle".to_string()),
                        message_id: None,
                        user_id: None,
                        group_id: None,
                        message: None,
                        raw_message: None,
                        sender: None,
                        raw: Some(serde_json::to_value(lifecycle).ok()?),
                        request_type: None,
                        comment: None,
                        flag: None,
                    })
                }
                runbot::event::MetaEvent::Heartbeat(heartbeat) => {
                    Some(OneBotMessage {
                        time: heartbeat.time,
                        self_id: heartbeat.self_id,
                        post_type: "meta_event".to_string(),
                        message_type: None,
                        sub_type: Some("heartbeat".to_string()),
                        message_id: None,
                        user_id: None,
                        group_id: None,
                        message: None,
                        raw_message: None,
                        sender: None,
                        raw: Some(serde_json::to_value(heartbeat).ok()?),
                        request_type: None,
                        comment: None,
                        flag: None,
                    })
                }
            }
        }
        runbot::event::Post::MessageSent(msg) => {
            // 发送的消息
            Some(OneBotMessage {
                time: msg.time,
                self_id,
                post_type: "message_sent".to_string(),
                message_type: Some(match &msg.message_type {
                    runbot::event::MessageType::Private => "private".to_string(),
                    runbot::event::MessageType::Group => "group".to_string(),
                    runbot::event::MessageType::Unknown(s) => s.clone(),
                }),
                sub_type: None,
                message_id: Some(msg.message_id),
                user_id: Some(msg.user_id),
                group_id: match &msg.message_type {
                    runbot::event::MessageType::Group => Some(msg.group_id),
                    _ => None,
                },
                message: Some(msg.raw_message.clone()),
                raw_message: Some(msg.raw_message.clone()),
                sender: None,
                raw: Some(serde_json::to_value(msg).ok()?),
                request_type: None,
                comment: None,
                flag: None,
            })
        }
        runbot::event::Post::Unknown(_) => None,
    }
}

// Tauri 事件转发 Processor
#[derive(Debug)]
struct TauriEventProcessor {
    app: AppHandle,
    state: Arc<Mutex<RunbotState>>,
}

#[async_trait]
impl PostProcessor for TauriEventProcessor {
    fn id(&self) -> &'static str {
        "tauri_event_forwarder"
    }

    async fn process_post(
        &self,
        bot_ctx: Arc<BotContext>,
        post: &runbot::event::Post,
    ) -> anyhow::Result<bool> {
        tracing::debug!("[TauriEventProcessor {:p}] 收到 Post: {:?}", self as *const _, post);
        
        // 从 Post 中提取 self_id（优先从消息中获取）
        let mut self_id = bot_ctx.id;
        tracing::debug!("[TauriEventProcessor] bot_ctx.id = {}", self_id);
        
        // 尝试从消息中提取 self_id
        match post {
            runbot::event::Post::Message(msg) => {
                if msg.self_id > 0 {
                    self_id = msg.self_id;
                    tracing::debug!("[TauriEventProcessor] 从 Message 中提取 self_id = {}", self_id);
                }
            }
            runbot::event::Post::MetaEvent(meta) => {
                match meta {
                    runbot::event::MetaEvent::Lifecycle(lifecycle) => {
                        if lifecycle.self_id > 0 {
                            self_id = lifecycle.self_id;
                            tracing::debug!("[TauriEventProcessor] 从 Lifecycle 中提取 self_id = {}", self_id);
                        }
                    }
                    runbot::event::MetaEvent::Heartbeat(heartbeat) => {
                        if heartbeat.self_id > 0 {
                            self_id = heartbeat.self_id;
                            tracing::debug!("[TauriEventProcessor] 从 Heartbeat 中提取 self_id = {}", self_id);
                        }
                    }
                }
            }
            _ => {}
        }
        
        // 更新 self_id
        {
            let mut state_guard = self.state.lock().unwrap();
            if state_guard.self_id.is_none() && self_id > 0 {
                tracing::info!("[TauriEventProcessor] 更新 self_id: {} -> {}", state_guard.self_id.unwrap_or(0), self_id);
                state_guard.self_id = Some(self_id);
                tracing::info!("[TauriEventProcessor] 发送 runbot-self-id 事件: {}", self_id);
                self.app
                    .emit("runbot-self-id", self_id)
                    .unwrap_or_default();
                
                // 静态变量会通过 lib.rs 中的事件监听器更新
            } else {
                tracing::debug!("[TauriEventProcessor] 跳过 self_id 更新: state.self_id = {:?}, 当前 self_id = {}", state_guard.self_id, self_id);
            }
        }
        
        // 转换并发送事件
        if let Some(message) = post_to_onebot_message(post, self_id, &self.app) {
            tracing::debug!(
                "发送 runbot-message 事件: post_type={}, message_type={:?}, message_id={:?}, raw_message={:?}",
                message.post_type,
                message.message_type,
                message.message_id,
                message.raw_message
            );
            self.app
                .emit("runbot-message", message)
                .unwrap_or_default();
        } else {
            tracing::warn!("[TauriEventProcessor] 无法转换 Post 为 OneBotMessage: {:?}", post);
        }
        
        Ok(false) // 不拦截，继续处理其他 processor
    }
}

/// 连接 Runbot OneBot v11 WebSocket 服务器
#[tauri::command]
pub async fn connect_runbot(
    ws_url: String,
    access_token: Option<String>,
    app: AppHandle,
    state: State<'_, Arc<Mutex<RunbotState>>>,
) -> Result<(), String> {
    // 如果已经连接，先断开并 shutdown 旧的 BotContext
    let old_bot_ctx = {
        let mut state_guard = state.lock().map_err(|e| format!("锁定状态失败: {}", e))?;
        if state_guard.connected {
            tracing::info!("[connect_runbot] 检测到已有连接，先断开并 shutdown 旧连接");
            
            // 取出旧的 BotContext（在释放锁之前）
            let old_bot_ctx = state_guard.bot_ctx.take();
            
            // 清理状态
            state_guard.connected = false;
            state_guard.self_id = None;
            
            old_bot_ctx
        } else {
            None
        }
    };
    
    // 在锁外调用 shutdown（避免持有锁时 await）
    if let Some(old_bot_ctx) = old_bot_ctx {
        tracing::info!("[connect_runbot] 正在 shutdown 旧的 BotContext");
        if let Err(e) = old_bot_ctx.shutdown().await {
            tracing::warn!("[connect_runbot] shutdown 旧 BotContext 失败: {}", e);
        } else {
            tracing::info!("[connect_runbot] 旧 BotContext shutdown 成功");
        }
    }

    // 构建 WebSocket URL（添加 access_token）
    let mut url = ws_url.clone();
    if let Some(token) = &access_token {
        url = format!("{}?access_token={}", url, token);
    }
    tracing::debug!("[connect_runbot] 构建的 URL: {}", url);

    // 创建 BotContext
    let app_clone = app.clone();
    let state_clone = state.inner().clone();
    
    // 创建 Tauri Processor（确保只创建一个实例）
    tracing::debug!("[connect_runbot] 创建 TauriEventProcessor (AppHandle 地址: {:p})", &app_clone as *const _);
    let processor = TauriEventProcessor {
        app: app_clone.clone(),
        state: state_clone.clone(),
    };
    
    tracing::debug!("[connect_runbot] 创建 BotContextBuilder");
    let bot_ctx = BotContextBuilder::new()
        .url(&url)
        .add_processor(Box::new(processor) as Box<dyn PostProcessor>)
        .build()
        .map_err(|e| format!("创建 BotContext 失败: {}", e))?;
    
    tracing::info!("[connect_runbot] BotContext 创建成功，初始 id = {}, BotContext 地址: {:p}", bot_ctx.id, &bot_ctx as *const _);

    // 更新状态
    {
        let mut state_guard = state.lock().map_err(|e| format!("锁定状态失败: {}", e))?;
        state_guard.connected = true;
        state_guard.ws_url = Some(ws_url.clone());
        state_guard.access_token = access_token.clone();
        state_guard.bot_ctx = Some(bot_ctx.clone());
        state_guard.app_handle = Some(app_clone.clone());
    }

    // 发送连接中状态
    tracing::debug!("[connect_runbot] 发送连接中状态");
    app.emit(
        "runbot-status",
        ConnectionStatus {
            status: "connecting".to_string(),
            message: Some("正在连接...".to_string()),
        },
    )
    .unwrap_or_default();

    // 启动 runbot 客户端循环（在后台任务中）
    let bot_ctx_clone = bot_ctx.clone();
    tracing::info!("[connect_runbot] 启动 runbot 客户端循环，URL: {}", url);
    tokio::spawn(async move {
        // 运行 runbot 客户端
        tracing::info!("[loop_client] 开始连接...");
        if let Err(e) = loop_client(bot_ctx_clone.clone()).await {
            tracing::error!("[loop_client] Runbot 客户端错误: {:?}", e);
            
            // 在错误情况下，确保 shutdown BotContext
            tracing::info!("[loop_client] 连接失败，正在 shutdown BotContext");
            if let Err(shutdown_err) = bot_ctx_clone.shutdown().await {
                tracing::warn!("[loop_client] shutdown BotContext 失败: {}", shutdown_err);
            } else {
                tracing::info!("[loop_client] BotContext shutdown 成功");
            }
            
            // 更新状态
            {
                let mut state_guard = state_clone.lock().unwrap();
                state_guard.connected = false;
                state_guard.bot_ctx = None;
            }
            
            // 发送错误状态
            app_clone
                .emit(
                    "runbot-status",
                    ConnectionStatus {
                        status: "error".to_string(),
                        message: Some(format!("连接错误: {}", e)),
                    },
                )
                .unwrap_or_default();
        }
    });

    // 等待一下，让连接建立
    tracing::debug!("[connect_runbot] 等待连接建立...");
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    // 主动获取 self_id（使用 get_login_info API）
    let bot_ctx_for_login = bot_ctx.clone();
    let app_for_login = app.clone();
    let state_for_login = state.inner().clone();
    
    tokio::spawn(async move {
        // 重试几次，因为连接可能还没完全建立
        let mut retries = 5;
        let mut delay = 500; // 初始延迟 500ms
        let mut login_success = false;
        
        while retries > 0 {
            tracing::debug!("[connect_runbot] 尝试获取登录信息，剩余重试次数: {}", retries);
            
            match bot_ctx_for_login.get_login_info().await {
                Ok(login_info) => {
                    let self_id = login_info.user_id;
                    tracing::info!("[connect_runbot] 成功获取 self_id: {} (昵称: {})", self_id, login_info.nickname);
                    
                    // 更新状态
                    {
                        let mut state_guard = state_for_login.lock().unwrap();
                        state_guard.self_id = Some(self_id);
                    }
                    
                    // 更新静态变量（用于协议处理器）
                    if let Some(current_self_id) = CURRENT_SELF_ID.get() {
                        let mut guard = current_self_id.lock().unwrap();
                        *guard = Some(self_id);
                    }
                    
                    // 发送 self_id 事件
                    app_for_login
                        .emit("runbot-self-id", self_id)
                        .unwrap_or_default();
                    
                    login_success = true;
                    break;
                }
                Err(e) => {
                    tracing::warn!("[connect_runbot] 获取登录信息失败 (剩余重试: {}): {}", retries - 1, e);
                    retries -= 1;
                    if retries > 0 {
                        tokio::time::sleep(tokio::time::Duration::from_millis(delay)).await;
                        delay *= 2; // 指数退避
                    }
                }
            }
        }
        
        // 如果登录失败（重试次数用完），shutdown BotContext
        if !login_success {
            tracing::error!("[connect_runbot] 获取登录信息失败，已达到最大重试次数，正在 shutdown BotContext");
            
            // 检查状态，如果连接仍然存在，则 shutdown
            let should_shutdown = {
                let state_guard = state_for_login.lock().unwrap();
                state_guard.connected && state_guard.bot_ctx.is_some()
            };
            
            if should_shutdown {
                if let Err(shutdown_err) = bot_ctx_for_login.shutdown().await {
                    tracing::warn!("[connect_runbot] shutdown BotContext 失败: {}", shutdown_err);
                } else {
                    tracing::info!("[connect_runbot] BotContext shutdown 成功");
                }
                
                // 更新状态
                {
                    let mut state_guard = state_for_login.lock().unwrap();
                    state_guard.connected = false;
                    state_guard.bot_ctx = None;
                    state_guard.self_id = None;
                }
                
                // 发送错误状态
                app_for_login
                    .emit(
                        "runbot-status",
                        ConnectionStatus {
                            status: "error".to_string(),
                            message: Some("获取登录信息失败，已断开连接".to_string()),
                        },
                    )
                    .unwrap_or_default();
            }
        }
    });

    // 发送连接成功状态
    tracing::info!("[connect_runbot] 发送连接成功状态");
    app.emit(
        "runbot-status",
        ConnectionStatus {
            status: "connected".to_string(),
            message: Some("已连接".to_string()),
        },
    )
    .unwrap_or_default();

    tracing::info!("[connect_runbot] 连接流程完成");
    Ok(())
}

/// 断开连接
#[tauri::command]
pub async fn disconnect_runbot(
    state: State<'_, Arc<Mutex<RunbotState>>>,
    app: AppHandle,
) -> Result<(), String> {
    // 获取旧的 BotContext 并 shutdown
    let old_bot_ctx = {
        let mut state_guard = state.lock().map_err(|e| format!("锁定状态失败: {}", e))?;
        state_guard.connected = false;
        state_guard.bot_ctx.take() // 取出 BotContext，这样状态中就没有了
    };
    
    // 如果存在 BotContext，先调用 shutdown
    if let Some(bot_ctx) = old_bot_ctx {
        tracing::info!("[disconnect_runbot] 正在 shutdown BotContext");
        if let Err(e) = bot_ctx.shutdown().await {
            tracing::warn!("[disconnect_runbot] shutdown BotContext 失败: {}", e);
        } else {
            tracing::info!("[disconnect_runbot] BotContext shutdown 成功");
        }
    }
    
    // 清理剩余状态
    {
        let mut state_guard = state.lock().map_err(|e| format!("锁定状态失败: {}", e))?;
        state_guard.self_id = None;
    }

    app.emit(
        "runbot-status",
        ConnectionStatus {
            status: "disconnected".to_string(),
            message: Some("已断开连接".to_string()),
        },
    )
    .unwrap_or_default();

    Ok(())
}

/// 获取连接状态
#[tauri::command]
pub async fn get_runbot_status(
    state: State<'_, Arc<Mutex<RunbotState>>>,
) -> Result<ConnectionStatus, String> {
    let state_guard = state.lock().map_err(|e| format!("锁定状态失败: {}", e))?;
    
    Ok(ConnectionStatus {
        status: if state_guard.connected {
            "connected".to_string()
        } else {
            "disconnected".to_string()
        },
        message: if state_guard.connected {
            Some("已连接".to_string())
        } else {
            Some("未连接".to_string())
        },
    })
}

/// 获取当前 self_id（QQ 号）
#[tauri::command]
pub async fn get_runbot_self_id(
    state: State<'_, Arc<Mutex<RunbotState>>>,
) -> Result<Option<i64>, String> {
    let state_guard = state.lock().map_err(|e| format!("锁定状态失败: {}", e))?;
    Ok(state_guard.self_id)
}

/// 发送消息到 Runbot（调用 OneBot API）
#[tauri::command]
pub async fn send_runbot_message(
    action: String,
    params: serde_json::Value,
    state: State<'_, Arc<Mutex<RunbotState>>>,
    app: AppHandle,
) -> Result<(), String> {
    
    let (bot_ctx, self_id) = {
        let state_guard = state.lock().map_err(|e| format!("锁定状态失败: {}", e))?;
        
        if !state_guard.connected {
            return Err("未连接到 Runbot 服务器".to_string());
        }

        (state_guard.bot_ctx.clone(), state_guard.self_id)
    };

    if let Some(bot_ctx) = bot_ctx {
        // 对于 send_private_msg 和 send_group_msg，如果 message 是数组格式，使用 runbot 的 API 方法
        if action == "send_private_msg" || action == "send_group_msg" {
            // 检查 message 字段
            if let Some(message_value) = params.get("message") {
                // 如果 message 是数组格式，转换为 MessageChain
                if let Some(message_array) = message_value.as_array() {
                    // 解析为 MessageChain
                    let message_chain: runbot::event::MessageChain = message_array
                        .iter()
                        .map(|item| runbot::event::MessageData::parse(item))
                        .collect::<std::result::Result<Vec<_>, _>>()
                        .map_err(|e| format!("解析消息数组失败: {}", e))?;
                    
                    // 获取 local_message_id（如果提供）
                    let local_message_id = params.get("local_message_id")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());
                    
                    // 获取 need_reload 参数（如果消息包含图片，需要重新加载）
                    let need_reload = params.get("need_reload")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);
                    
                    // 使用 runbot 的 API 方法发送
                    if action == "send_private_msg" {
                        if let Some(user_id) = params.get("user_id").and_then(|v| v.as_i64()) {
                            let response = bot_ctx
                                .send_private_message(user_id, message_chain)
                                .await
                                .map_err(|e| format!("发送私聊消息失败: {}", e))?;
                            
                            // 如果有 local_message_id，等待响应并更新数据库
                            if let Some(local_id) = local_message_id {
                                // 在后台任务中等待响应并更新数据库
                                let app_clone = app.clone();
                                let local_id_clone = local_id.clone();
                                let bot_ctx_clone = bot_ctx.clone();
                                tokio::spawn(async move {
                                    if let Ok(send_response) = response.wait_response().await {
                                        // 更新数据库中的 message_id
                                        if let Err(e) = crate::storage::update_message_id(
                                            local_id_clone.clone(),
                                            send_response.message_id,
                                            self_id,
                                            app_clone.clone(),
                                        ).await {
                                            tracing::warn!("更新消息 message_id 失败: {}", e);
                                        } else {
                                            // 如果需要重新加载（包含图片），获取完整消息内容
                                            if need_reload {
                                                if let Ok(full_message) = bot_ctx_clone.get_msg(send_response.message_id).await {
                                                    // 将 Message 转换为 CQ 码格式的字符串
                                                    let message_str = full_message.raw_message.clone(); // message_to_cqcode(&full_message);
                                                    
                                                    // 更新数据库中的消息内容
                                                    if let Err(e) = crate::storage::update_message_content(
                                                        local_id_clone.clone(),
                                                        message_str.clone(),
                                                        message_str.clone(),
                                                        self_id,
                                                        app_clone.clone(),
                                                    ).await {
                                                        tracing::warn!("更新消息内容失败: {}", e);
                                                    }
                                                    
                                                    // 发送事件通知前端更新消息内容
                                                    #[derive(serde::Serialize, Clone)]
                                                    struct MessageUpdatedEvent {
                                                        local_message_id: String,
                                                        message_id: i64,
                                                        message: String,
                                                        raw_message: String,
                                                    }
                                                    let _ = app_clone.emit("message-updated", MessageUpdatedEvent {
                                                        local_message_id: local_id_clone.clone(),
                                                        message_id: send_response.message_id,
                                                        message: message_str.clone(),
                                                        raw_message: message_str,
                                                    });
                                                } else {
                                                    tracing::warn!("获取完整消息内容失败: message_id={}", send_response.message_id);
                                                }
                                            }
                                            
                                            // 发送事件通知前端消息已发送成功
                                            #[derive(serde::Serialize, Clone)]
                                            struct MessageSentEvent {
                                                local_message_id: String,
                                                message_id: i64,
                                            }
                                            let _ = app_clone.emit("message-sent", MessageSentEvent {
                                                local_message_id: local_id_clone,
                                                message_id: send_response.message_id,
                                            });
                                        }
                                    }
                                });
                            }
                            
                            return Ok(());
                        }
                    } else if action == "send_group_msg" {
                        if let Some(group_id) = params.get("group_id").and_then(|v| v.as_i64()) {
                            let response = bot_ctx
                                .send_group_message(group_id, message_chain)
                                .await
                                .map_err(|e| format!("发送群消息失败: {}", e))?;
                            
                            // 如果有 local_message_id，等待响应并更新数据库
                            if let Some(local_id) = local_message_id {
                                // 在后台任务中等待响应并更新数据库
                                let app_clone = app.clone();
                                let local_id_clone = local_id.clone();
                                let bot_ctx_clone = bot_ctx.clone();
                                tokio::spawn(async move {
                                    if let Ok(send_response) = response.wait_response().await {
                                        // 更新数据库中的 message_id
                                        if let Err(e) = crate::storage::update_message_id(
                                            local_id_clone.clone(),
                                            send_response.message_id,
                                            self_id,
                                            app_clone.clone(),
                                        ).await {
                                            tracing::warn!("更新消息 message_id 失败: {}", e);
                                        } else {
                                            // 如果需要重新加载（包含图片），获取完整消息内容
                                            if need_reload {
                                                if let Ok(full_message) = bot_ctx_clone.get_msg(send_response.message_id).await {
                                                    // 将 Message 转换为 CQ 码格式的字符串
                                                    let message_str = full_message.raw_message.clone(); // message_to_cqcode(&full_message);
                                                    
                                                    // 更新数据库中的消息内容
                                                    if let Err(e) = crate::storage::update_message_content(
                                                        local_id_clone.clone(),
                                                        message_str.clone(),
                                                        message_str.clone(),
                                                        self_id,
                                                        app_clone.clone(),
                                                    ).await {
                                                        tracing::warn!("更新消息内容失败: {}", e);
                                                    }
                                                    
                                                    // 发送事件通知前端更新消息内容
                                                    #[derive(serde::Serialize, Clone)]
                                                    struct MessageUpdatedEvent {
                                                        local_message_id: String,
                                                        message_id: i64,
                                                        message: String,
                                                        raw_message: String,
                                                    }
                                                    let _ = app_clone.emit("message-updated", MessageUpdatedEvent {
                                                        local_message_id: local_id_clone.clone(),
                                                        message_id: send_response.message_id,
                                                        message: message_str.clone(),
                                                        raw_message: message_str,
                                                    });
                                                } else {
                                                    tracing::warn!("获取完整消息内容失败: message_id={}", send_response.message_id);
                                                }
                                            }
                                            
                                            // 发送事件通知前端消息已发送成功
                                            #[derive(serde::Serialize, Clone)]
                                            struct MessageSentEvent {
                                                local_message_id: String,
                                                message_id: i64,
                                            }
                                            let _ = app_clone.emit("message-sent", MessageSentEvent {
                                                local_message_id: local_id_clone,
                                                message_id: send_response.message_id,
                                            });
                                        }
                                    }
                                });
                            }
                            
                            return Ok(());
                        }
                    }
                }
            }
        }
        
        // 对于其他 API 或字符串格式的消息，直接使用 websocket_send
        bot_ctx
            .websocket_send(&action, params)
            .await
            .map_err(|e| format!("发送消息失败: {}", e))?;
        
        Ok(())
    } else {
        Err("BotContext 不存在".to_string())
    }
}

/// 获取合并转发消息内容
#[tauri::command]
pub async fn get_forward_message(
    id: String,
    state: State<'_, Arc<Mutex<RunbotState>>>,
) -> Result<serde_json::Value, String> {
    let bot_ctx = {
        let state_guard = state.lock().map_err(|e| format!("锁定状态失败: {}", e))?;
        
        if !state_guard.connected {
            return Err("未连接到 Runbot 服务器".to_string());
        }

        state_guard.bot_ctx.clone()
    };

    if let Some(bot_ctx) = bot_ctx {
        // 直接调用 websocket_send 获取原始响应，不进行解析
        let response = bot_ctx
            .websocket_send("get_forward_msg", serde_json::json!({
                "id": id,
            }))
            .await
            .map_err(|e| format!("发送获取合并转发消息请求失败: {}", e))?;
        
        // 等待响应数据
        let data = response
            .data(tokio::time::Duration::from_secs(10))
            .await
            .map_err(|e| format!("获取合并转发消息响应失败: {}", e))?;
        
        // 直接返回原始 JSON 数据
        Ok(data)
    } else {
        Err("未连接到 Runbot 服务器".to_string())
    }
}

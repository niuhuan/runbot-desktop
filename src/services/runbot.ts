import { invoke } from '@tauri-apps/api/core';
import { listen, UnlistenFn } from '@tauri-apps/api/event';

export interface ConnectionStatus {
  status: 'connected' | 'disconnected' | 'connecting' | 'error';
  message?: string;
}

export interface OneBotMessage {
  localMessageId?: string; // 本地唯一标识，作为主键
  time: number;
  self_id: number;
  post_type: string;
  message_type?: string;
  sub_type?: string;
  message_id?: number;
  user_id?: number;
  group_id?: number;
  message?: string;
  raw_message?: string;
  sender?: any;
  raw?: any;
  // API 响应字段
  status?: string;
  retcode?: number;
  data?: any;
  echo?: string;
  action?: string;
  // 消息发送状态
  sendStatus?: 'sending' | 'sent' | 'failed';
  // 撤回状态
  recalled?: boolean;
  // 通知类型相关
  notice_type?: string;  // 通知类型: group_recall, friend_recall等
  operator_id?: number;  // 操作者ID（撤回消息的人）
  // 请求相关字段
  request_type?: string; // 请求类型: friend, group
  comment?: string;      // 验证消息/请求说明
  flag?: string;         // 请求标识，用于处理请求
}

class RunbotService {
  private statusListeners: UnlistenFn[] = [];
  private messageListeners: UnlistenFn[] = [];
  private isConnected = false;
  private apiResponseCallbacks: Map<string, (data: any) => void> = new Map();

  /**
   * 连接 Runbot WebSocket 服务器
   */
  async connect(wsUrl: string, accessToken?: string): Promise<void> {
    try {
      await invoke('connect_runbot', {
        wsUrl,
        accessToken: accessToken || null,
      });
      this.isConnected = true;
    } catch (error) {
      this.isConnected = false;
      throw error;
    }
  }

  /**
   * 断开连接
   */
  async disconnect(): Promise<void> {
    try {
      await invoke('disconnect_runbot');
      this.isConnected = false;
      this.cleanupListeners();
    } catch (error) {
      throw error;
    }
  }

  /**
   * 获取连接状态
   */
  async getConnectionStatus(): Promise<ConnectionStatus> {
    return await invoke('get_runbot_status');
  }

  /**
   * 获取当前 self_id（QQ 号）
   */
  async getSelfId(): Promise<number | null> {
    return await invoke<number | null>('get_runbot_self_id');
  }

  /**
   * 发送 OneBot API 请求
   */
  async sendMessage(action: string, params: Record<string, any>): Promise<void> {
    if (!this.isConnected) {
      throw new Error('未连接到 Runbot 服务器');
    }

    await invoke('send_runbot_message', {
      action,
      params,
    });
  }

  /**
   * 发送 OneBot API 请求并等待响应
   */
  async sendMessageWithResponse(action: string, params: Record<string, any>, timeout = 10000): Promise<any> {
    if (!this.isConnected) {
      throw new Error('未连接到 Runbot 服务器');
    }

    // 生成唯一的 echo 标识
    const echo = `${action}-${Date.now()}-${Math.random().toString(36).substring(7)}`;

    // 创建一个 Promise 来等待响应
    return new Promise((resolve, reject) => {
      const timeoutId = setTimeout(() => {
        this.apiResponseCallbacks.delete(echo);
        reject(new Error(`API 请求超时: ${action}`));
      }, timeout);

      // 注册回调
      this.apiResponseCallbacks.set(echo, (data: any) => {
        clearTimeout(timeoutId);
        this.apiResponseCallbacks.delete(echo);
        resolve(data);
      });

      // 发送请求（带 echo）
      invoke('send_runbot_message', {
        action,
        params: { ...params, echo },
      }).catch((error) => {
        clearTimeout(timeoutId);
        this.apiResponseCallbacks.delete(echo);
        reject(error);
      });
    });
  }

  /**
   * 处理 API 响应（需要从外部调用）
   */
  handleApiResponse(message: OneBotMessage): void {
    // 尝试从 message.echo 或 message.raw.echo 获取 echo
    const echo = message.echo || (message.raw && typeof message.raw === 'object' ? (message.raw as any).echo : null);
    
    console.log('[RunbotService] handleApiResponse:', { echo, hasCallback: echo ? this.apiResponseCallbacks.has(echo) : false });
    
    if (echo && this.apiResponseCallbacks.has(echo)) {
      const callback = this.apiResponseCallbacks.get(echo);
      if (callback) {
        // 尝试从不同位置获取 data
        const data = message.data || (message.raw && typeof message.raw === 'object' ? (message.raw as any).data : null);
        console.log('[RunbotService] 调用 API 响应回调, echo:', echo, 'data:', data);
        callback(data);
      }
    }
  }

  /**
   * 监听连接状态变化
   */
  async onStatusChange(callback: (status: ConnectionStatus) => void): Promise<UnlistenFn> {
    const unlisten = await listen<ConnectionStatus>('runbot-status', (event) => {
      console.log('runbot-status', event.payload);
      const status = event.payload.status;
      if (status === 'connected') {
        this.isConnected = true;
      } else if (status === 'disconnected' || status === 'error') {
        this.isConnected = false;
      }
      callback(event.payload);
    });

    this.statusListeners.push(unlisten);
    return unlisten;
  }

  /**
   * 监听消息
   */
  async onMessage(callback: (message: OneBotMessage) => void): Promise<UnlistenFn> {
    const unlisten = await listen<OneBotMessage>('runbot-message', (event) => {
      callback(event.payload);
    });

    this.messageListeners.push(unlisten);
    return unlisten;
  }

  /**
   * 监听原始消息（非标准格式）
   */
  async onRawMessage(callback: (message: string) => void): Promise<UnlistenFn> {
    const unlisten = await listen<string>('runbot-raw-message', (event) => {
      callback(event.payload);
    });

    this.messageListeners.push(unlisten);
    return unlisten;
  }

  /**
   * 清理所有监听器
   */
  private cleanupListeners(): void {
    this.statusListeners.forEach((unlisten) => unlisten());
    this.messageListeners.forEach((unlisten) => unlisten());
    this.statusListeners = [];
    this.messageListeners = [];
  }

  /**
   * 获取连接状态
   */
  get connected(): boolean {
    return this.isConnected;
  }

  // ========== 常用 OneBot API 封装方法 ==========

  /**
   * 获取机器人信息
   */
  async getSelfInfo(): Promise<void> {
    await this.sendMessage('get_self_info', {});
  }

  /**
   * 获取机器人状态
   */
  async getStatus(): Promise<void> {
    await this.sendMessage('get_status', {});
  }

  /**
   * 获取用户状态（在线、隐身、离线等）
   */
  async getUserStatus(userId: number): Promise<void> {
    await this.sendMessage('get_user_status', {
      user_id: userId,
    });
  }

  /**
   * 设置在线状态
   */
  async setOnlineStatus(status: number): Promise<void> {
    await this.sendMessage('set_online_status', {
      status: status,
    });
  }

  /**
   * 获取在线模式
   */
  async getOnlineModel(): Promise<void> {
    await this.sendMessage('get_online_model', {});
  }

  /**
   * 设置在线模式
   */
  async setOnlineModel(model: string): Promise<void> {
    await this.sendMessage('set_online_model', {
      model: model,
    });
  }

  /**
   * 发送私聊消息
   * @param userId 用户 ID
   * @param message 消息内容，可以是字符串（CQ 码格式）或数组（NapCat API 格式）
   * @param localMessageId 本地消息 ID，用于后续更新 message_id
   * @param autoEscape 是否自动转义（仅对字符串格式有效）
   */
  async sendPrivateMessage(
    userId: number, 
    message: string | Array<{ type: string; data: Record<string, any> }>, 
    localMessageId?: string,
    autoEscape: boolean = true,
    needReload: boolean = false
  ): Promise<void> {
    const params: Record<string, any> = {
      user_id: userId,
      message: message,
      auto_escape: autoEscape,
    };
    if (localMessageId) {
      params.local_message_id = localMessageId;
    }
    if (needReload) {
      params.need_reload = true;
    }
    await this.sendMessage('send_private_msg', params);
  }

  /**
   * 发送群消息
   * @param groupId 群组 ID
   * @param message 消息内容，可以是字符串（CQ 码格式）或数组（NapCat API 格式）
   * @param localMessageId 本地消息 ID，用于后续更新 message_id
   * @param autoEscape 是否自动转义（仅对字符串格式有效）
   */
  async sendGroupMessage(
    groupId: number, 
    message: string | Array<{ type: string; data: Record<string, any> }>, 
    localMessageId?: string,
    autoEscape: boolean = true,
    needReload: boolean = false
  ): Promise<void> {
    const params: Record<string, any> = {
      group_id: groupId,
      message: message,
      auto_escape: autoEscape,
    };
    if (localMessageId) {
      params.local_message_id = localMessageId;
    }
    if (needReload) {
      params.need_reload = true;
    }
    await this.sendMessage('send_group_msg', params);
  }

  /**
   * 获取好友列表
   */
  async getFriendList(): Promise<void> {
    await this.sendMessage('get_friend_list', {});
  }

  /**
   * 获取群列表
   */
  async getGroupList(): Promise<void> {
    await this.sendMessage('get_group_list', {});
  }

  /**
   * 获取群信息
   */
  async getGroupInfo(groupId: number, noCache: boolean = false): Promise<void> {
    await this.sendMessage('get_group_info', {
      group_id: groupId,
      no_cache: noCache,
    });
  }

  /**
   * 获取群成员列表
   */
  async getGroupMemberList(groupId: number): Promise<void> {
    console.log(`[RunbotService] 请求获取群 ${groupId} 的成员列表`);
    await this.sendMessage('get_group_member_list', {
      group_id: groupId,
    });
  }

  /**
   * 获取群成员信息
   */
  async getGroupMemberInfo(groupId: number, userId: number, noCache: boolean = false): Promise<void> {
    await this.sendMessage('get_group_member_info', {
      group_id: groupId,
      user_id: userId,
      no_cache: noCache,
    });
  }

  /**
   * 撤回消息
   */
  async deleteMessage(messageId: number): Promise<void> {
    await this.sendMessage('delete_msg', {
      message_id: messageId,
    });
  }

  /**
   * 获取消息
   */
  async getMsg(messageId: number): Promise<void> {
    await this.sendMessage('get_msg', {
      message_id: messageId,
    });
  }

  /**
   * 获取历史消息（私聊）
   */
  async getPrivateHistoryMessage(userId: number, messageSeq?: number): Promise<void> {
    await this.sendMessage('get_private_msg_history', {
      user_id: userId,
      message_seq: messageSeq,
    });
  }

  /**
   * 获取历史消息（群聊）
   */
  async getGroupHistoryMessage(groupId: number, messageSeq?: number): Promise<void> {
    await this.sendMessage('get_group_msg_history', {
      group_id: groupId,
      message_seq: messageSeq,
    });
  }

  /**
   * 设置群名
   */
  async setGroupName(groupId: number, groupName: string): Promise<void> {
    await this.sendMessage('set_group_name', {
      group_id: groupId,
      group_name: groupName,
    });
  }

  /**
   * 设置群成员名片
   */
  async setGroupCard(groupId: number, userId: number, card: string): Promise<void> {
    await this.sendMessage('set_group_card', {
      group_id: groupId,
      user_id: userId,
      card: card,
    });
  }

  /**
   * 群组踢人
   */
  async setGroupKick(groupId: number, userId: number, rejectAddRequest: boolean = false): Promise<void> {
    await this.sendMessage('set_group_kick', {
      group_id: groupId,
      user_id: userId,
      reject_add_request: rejectAddRequest,
    });
  }

  /**
   * 群组禁言
   */
  async setGroupBan(groupId: number, userId: number, duration: number = 30 * 60): Promise<void> {
    await this.sendMessage('set_group_ban', {
      group_id: groupId,
      user_id: userId,
      duration: duration,
    });
  }

  /**
   * 设置群管理员
   */
  async setGroupAdmin(groupId: number, userId: number, enable: boolean = true): Promise<void> {
    await this.sendMessage('set_group_admin', {
      group_id: groupId,
      user_id: userId,
      enable: enable,
    });
  }

  /**
   * 处理好友请求
   * @param flag 请求标识
   * @param approve 是否同意（true: 同意, false: 拒绝）
   * @param remark 好友备注（仅在同意时有效）
   */
  async setFriendAddRequest(flag: string, approve: boolean, remark: string = ''): Promise<void> {
    await this.sendMessage('set_friend_add_request', {
      flag: flag,
      approve: approve,
      remark: remark,
    });
  }

  /**
   * 处理群组请求
   * @param flag 请求标识
   * @param sub_type 请求类型（add: 申请入群, invite: 邀请入群）
   * @param approve 是否同意（true: 同意, false: 拒绝）
   * @param reason 拒绝理由（仅在拒绝时有效）
   */
  async setGroupAddRequest(
    flag: string,
    subType: string,
    approve: boolean,
    reason: string = ''
  ): Promise<void> {
    await this.sendMessage('set_group_add_request', {
      flag: flag,
      sub_type: subType,
      approve: approve,
      reason: reason,
    });
  }

  /**
   * 获取合并转发消息内容
   * @param id 合并转发消息 ID
   */
  async getForwardMessage(id: string): Promise<any> {
    console.log('[RunbotService] getForwardMessage 开始, id:', id);
    try {
      // 直接调用 Rust 后端的方法
      const result = await invoke('get_forward_message', { id });
      console.log('[RunbotService] getForwardMessage 成功, result:', result);
      return result;
    } catch (error) {
      console.error('[RunbotService] getForwardMessage 失败:', error);
      throw error;
    }
  }
}

export const runbotService = new RunbotService();


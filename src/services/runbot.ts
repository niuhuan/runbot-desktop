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
}

class RunbotService {
  private statusListeners: UnlistenFn[] = [];
  private messageListeners: UnlistenFn[] = [];
  private isConnected = false;

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
}

export const runbotService = new RunbotService();


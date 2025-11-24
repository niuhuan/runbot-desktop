/**
 * 存储服务
 * 使用 Rust 存储（rusqlite + FTS5）存储消息
 */

import { invoke } from '@tauri-apps/api/core';
import type { OneBotMessage } from './runbot';

/**
 * 保存消息到数据库（用户特定）
 */
export async function saveMessage(message: OneBotMessage, selfId?: number): Promise<string> {
  try {
    const localMessageId = await invoke<string>('save_message', {
      messageData: JSON.stringify(message),
      selfId: selfId || null,
    });
    return localMessageId;
  } catch (error) {
    console.error('保存消息失败:', error);
    throw error;
  }
}

export interface GetMessagesOptions {
  limit?: number;
  offset?: number;
  postType?: string;
  userId?: number;
  groupId?: number;
  selfId?: number;
}

/**
 * 获取消息列表（用户特定）
 */
export async function getMessages(options: GetMessagesOptions = {}): Promise<OneBotMessage[]> {
  try {
    const messages = await invoke<string[]>('get_messages', {
      limit: options.limit,
      offset: options.offset,
      postType: options.postType,
      userId: options.userId,
      groupId: options.groupId,
      selfId: options.selfId || null,
    });
    
    return messages.map(msg => JSON.parse(msg) as OneBotMessage);
  } catch (error) {
    console.error('获取消息失败:', error);
    throw error;
  }
}

export interface SearchMessagesOptions {
  query: string;
  limit?: number;
  offset?: number;
  selfId?: number;
}

/**
 * 搜索消息（全文搜索，用户特定）
 */
export async function searchMessages(options: SearchMessagesOptions): Promise<OneBotMessage[]> {
  try {
    const messages = await invoke<string[]>('search_messages', {
      query: options.query,
      limit: options.limit,
      offset: options.offset,
      selfId: options.selfId || null,
    });
    
    return messages.map(msg => JSON.parse(msg) as OneBotMessage);
  } catch (error) {
    console.error('搜索消息失败:', error);
    throw error;
  }
}

/**
 * 删除消息（用户特定）
 */
export async function deleteMessage(localMessageId: string, selfId?: number): Promise<void> {
  try {
    await invoke('delete_message', {
      localMessageId,
      selfId: selfId || null,
    });
  } catch (error) {
    console.error('删除消息失败:', error);
    throw error;
  }
}

/**
 * 清理旧消息（保留最近 N 条，用户特定）
 */
export async function cleanupOldMessages(keepCount: number, selfId?: number): Promise<number> {
  try {
    const deletedCount = await invoke<number>('cleanup_old_messages', {
      keepCount,
      selfId: selfId || null,
    });
    return deletedCount;
  } catch (error) {
    console.error('清理旧消息失败:', error);
    throw error;
  }
}

/**
 * 获取消息统计信息（用户特定）
 */
export async function getMessageStats(selfId?: number): Promise<{
  total: number;
  messages: number;
  notices: number;
}> {
  try {
    return await invoke('get_message_stats', {
      selfId: selfId || null,
    });
  } catch (error) {
    console.error('获取消息统计失败:', error);
    throw error;
  }
}

/**
 * 标记消息为已撤回
 */
export async function markMessageRecalled(messageId: number, selfId?: number): Promise<void> {
  try {
    await invoke('mark_message_recalled', {
      messageId,
      selfId: selfId || null,
    });
  } catch (error) {
    console.error('标记消息为已撤回失败:', error);
    throw error;
  }
}

/**
 * 检查消息是否已撤回（调试用）
 */
export async function checkMessageRecalled(messageId: number, selfId?: number): Promise<any> {
  try {
    const result = await invoke('check_message_recalled', {
      messageId,
      selfId: selfId || null,
    });
    return result;
  } catch (error) {
    console.error('检查消息撤回状态失败:', error);
    throw error;
  }
}

/**
 * 通过 message_id 获取消息
 */
export async function getMessageById(messageId: number, selfId?: number): Promise<OneBotMessage | null> {
  try {
    const result = await checkMessageRecalled(messageId, selfId);
    if (result.error) {
      return null;
    }
    // 从数据库查询完整的消息数据
    const messages = await getMessages({
      limit: 1,
      selfId: selfId
    });
    // 找到匹配的消息
    const message = messages.find(m => m.message_id === messageId);
    return message || null;
  } catch (error) {
    console.error('获取消息失败:', error);
    return null;
  }
}

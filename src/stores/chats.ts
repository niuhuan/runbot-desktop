/**
 * 全局对话列表管理
 * 统一维护对话列表，供所有组件使用
 */

import { reactive } from 'vue';
import { getMessages } from '../services/storage';
import { parseCQCode } from '../utils/cqcode';
import { getFaceDisplayText } from '../utils/qq-face';
import { getContactName, getGroupName } from './contacts';
import type { OneBotMessage } from '../services/runbot';

export interface ChatItem {
  id: string;
  type: 'private' | 'group';
  name: string;
  avatar?: string;
  avatarFailed?: boolean; // 标记头像是否加载失败
  lastMessage?: string;
  lastTime?: number;
  unreadCount: number;
  userId?: number;
  groupId?: number;
}

// 全局状态
const state = reactive<{
  chats: ChatItem[];
  initialized: boolean;
}>({
  chats: [],
  initialized: false,
});

/**
 * 格式化消息预览（将图片和表情替换为文本）
 */
function formatMessagePreview(message: string): string {
  if (!message) return '';
  
  const segments = parseCQCode(message);
  const parts: string[] = [];
  
  for (const segment of segments) {
    if (segment.type === 'text' && segment.text) {
      parts.push(segment.text);
    } else if (segment.type === 'image') {
      // 检查是否是动画表情（sub_type=1 或有 summary）
      const subType = segment.data.sub_type || '0';
      const summary = segment.data.summary || '';
      
      // 解码 HTML 实体
      const decodedSummary = summary
        .replace(/&#91;/g, '[')
        .replace(/&#93;/g, ']')
        .replace(/&amp;/g, '&')
        .replace(/&lt;/g, '<')
        .replace(/&gt;/g, '>')
        .replace(/&quot;/g, '"')
        .replace(/&#39;/g, "'");
      
      if (subType === '1' || decodedSummary.includes('动画表情') || decodedSummary.includes('表情')) {
        // 动画表情：有摘要显示摘要，否则显示 [动画表情]
        if (decodedSummary) {
          parts.push(`[${decodedSummary}]`);
        } else {
          parts.push('[动画表情]');
        }
      } else {
        // 普通图片
        parts.push('[图片]');
      }
    } else if (segment.type === 'face') {
      // 使用表情 ID 获取表情名称
      const faceId = segment.data.id || '';
      parts.push(getFaceDisplayText(faceId));
    } else if (segment.type === 'at') {
      parts.push(`@${segment.data.qq || ''}`);
    }
  }
  
  return parts.join('');
}

/**
 * 从消息中更新对话列表
 */
export async function updateChatList(selfId?: number): Promise<void> {
  // 如果没有 selfId，清空列表（等待连接）
  if (!selfId) {
    state.chats = [];
    return;
  }

  try {
    // 获取最近的消息（按用户/群组分组）
    const messages = await getMessages({ 
      limit: 500, // 减少到 500 条，通常足够显示所有聊天
      selfId 
    });

    // 按用户/群组分组
    const chatMap = new Map<string, ChatItem>();

    messages.forEach((msg) => {
      let chatId: string;
      let chatName: string;
      let chatType: 'private' | 'group';

      if (msg.message_type === 'private' && msg.user_id) {
        chatId = `private_${msg.user_id}`;
        // 优先从联系人列表获取名称，其次从消息的 sender，最后使用默认格式
        chatName = getContactName(msg.user_id);
        chatType = 'private';
      } else if (msg.message_type === 'group' && msg.group_id) {
        chatId = `group_${msg.group_id}`;
        // 优先从群组列表获取名称，其次尝试从消息的 raw 中获取，最后使用默认格式
        chatName = getGroupName(msg.group_id);
        chatType = 'group';
      } else {
        return; // 跳过其他类型的消息
      }

      if (!chatMap.has(chatId)) {
        const chatItem: ChatItem = {
          id: chatId,
          type: chatType,
          name: chatName,
          unreadCount: 0,
          userId: msg.user_id || undefined,
          groupId: msg.group_id || undefined,
        };
        
        // 设置头像 URL
        if (chatType === 'private' && msg.user_id) {
          chatItem.avatar = `asset://avatar/user/${msg.user_id}.png`;
        } else if (chatType === 'group' && msg.group_id) {
          chatItem.avatar = `asset://avatar/group/${msg.group_id}.png`;
        }
        
        chatMap.set(chatId, chatItem);
      } else {
        // 如果已存在，更新名称（可能联系人/群组列表已更新）
        const existingChat = chatMap.get(chatId)!;
        if (chatType === 'private' && msg.user_id) {
          existingChat.name = getContactName(msg.user_id);
        } else if (chatType === 'group' && msg.group_id) {
          existingChat.name = getGroupName(msg.group_id);
        }
      }

      const chat = chatMap.get(chatId)!;
      
      // 更新最后一条消息和时间
      if (!chat.lastTime || msg.time > chat.lastTime) {
        chat.lastTime = msg.time;
        const rawMessage = msg.message || msg.raw_message || '';
        chat.lastMessage = formatMessagePreview(rawMessage);
      }
    });

    // 转换为数组并按时间排序
    state.chats = Array.from(chatMap.values()).sort((a, b) => {
      const timeA = a.lastTime || 0;
      const timeB = b.lastTime || 0;
      return timeB - timeA;
    });
    console.log('[ChatsStore] 更新聊天列表成功:', state.chats.length);
  } catch (error) {
    console.error('更新聊天列表失败:', error);
  }
}

/**
 * 根据消息更新对话列表中的单个对话
 */
export function updateChatFromMessage(message: OneBotMessage, selfId?: number): void {
  if (!selfId) return;

  let chatId: string;
  let chatName: string;
  let chatType: 'private' | 'group';

  if (message.message_type === 'private' && message.user_id) {
    chatId = `private_${message.user_id}`;
    chatName = getContactName(message.user_id);
    chatType = 'private';
  } else if (message.message_type === 'group' && message.group_id) {
    chatId = `group_${message.group_id}`;
    chatName = getGroupName(message.group_id);
    chatType = 'group';
  } else {
    return; // 跳过其他类型的消息
  }

  // 查找或创建对话
  let chat = state.chats.find(c => c.id === chatId);
  
  if (!chat) {
    // 创建新对话
    chat = {
      id: chatId,
      type: chatType,
      name: chatName,
      unreadCount: 0,
      userId: message.user_id || undefined,
      groupId: message.group_id || undefined,
    };
    
    // 设置头像
    if (chatType === 'private' && message.user_id) {
      chat.avatar = `asset://avatar/user/${message.user_id}.png`;
    } else if (chatType === 'group' && message.group_id) {
      chat.avatar = `asset://avatar/group/${message.group_id}.png`;
    }
    
    state.chats.push(chat);
  } else {
    // 更新名称（可能联系人/群组列表已更新）
    if (chatType === 'private' && message.user_id) {
      chat.name = getContactName(message.user_id);
    } else if (chatType === 'group' && message.group_id) {
      chat.name = getGroupName(message.group_id);
    }
  }

  // 更新最后一条消息和时间
  const rawMessage = message.message || message.raw_message || '';
  if (!chat.lastTime || message.time > chat.lastTime) {
    chat.lastTime = message.time;
    chat.lastMessage = formatMessagePreview(rawMessage);
  }

  // 重新排序（将更新的对话移到最前面）
  state.chats.sort((a, b) => {
    const timeA = a.lastTime || 0;
    const timeB = b.lastTime || 0;
    return timeB - timeA;
  });
}

/**
 * 根据对话ID获取对话
 */
export function getChat(chatId: string): ChatItem | undefined {
  return state.chats.find(c => c.id === chatId);
}

/**
 * 更新对话的头像加载失败状态
 */
export function setChatAvatarFailed(chatId: string, failed: boolean): void {
  const chat = state.chats.find(c => c.id === chatId);
  if (chat) {
    chat.avatarFailed = failed;
    if (failed) {
      chat.avatar = undefined;
    }
  }
}

/**
 * 获取只读的对话列表
 * 注意：readonly 不会影响响应式，但为了确保响应式正常工作，直接返回 state.chats
 * 因为 state 本身就是 reactive 的，所以 state.chats 也是响应式的
 */
export function useChatsState() {
  return {
    chats: state.chats, // 直接返回，保持响应式
    get initialized() {
      return state.initialized;
    },
  };
}

/**
 * 获取对话列表状态（响应式）
 */
export function getChatsState() {
  return state;
}

/**
 * 初始化对话列表管理
 */
export function initChatsStore(): void {
  state.initialized = true;
}


/**
 * 聊天输入状态管理
 * 为每个聊天保存独立的输入状态
 */

import { reactive } from 'vue';

// 单个聊天的输入状态
export interface ChatInputState {
  chatKey: string; // 'private-{userId}' 或 'group-{groupId}'
  editorHtml: string; // 富文本编辑器的 HTML 内容
  selectedImages: Array<{ file: File; preview: string }>; // 选中的图片
  mentionedUsers: Array<{ userId: number; displayName: string }>; // @的用户列表
  cursorPosition?: number; // 光标位置（如果需要）
}

// 所有聊天的输入状态映射
const chatInputStates = reactive<Map<string, ChatInputState>>(new Map());

/**
 * 生成聊天的唯一 key
 */
export function getChatKey(chatType: 'private' | 'group', chatId: number): string {
  return `${chatType}-${chatId}`;
}

/**
 * 获取指定聊天的输入状态
 */
export function getChatInputState(chatType: 'private' | 'group', chatId: number): ChatInputState {
  const chatKey = getChatKey(chatType, chatId);
  
  if (!chatInputStates.has(chatKey)) {
    // 创建新的输入状态
    chatInputStates.set(chatKey, {
      chatKey,
      editorHtml: '',
      selectedImages: [],
      mentionedUsers: [],
    });
  }
  
  return chatInputStates.get(chatKey)!;
}

/**
 * 更新指定聊天的输入状态
 */
export function updateChatInputState(
  chatType: 'private' | 'group',
  chatId: number,
  updates: Partial<Omit<ChatInputState, 'chatKey'>>
): void {
  const state = getChatInputState(chatType, chatId);
  Object.assign(state, updates);
}

/**
 * 清空指定聊天的输入状态
 */
export function clearChatInputState(chatType: 'private' | 'group', chatId: number): void {
  const state = getChatInputState(chatType, chatId);
  
  // 清理图片预览 URL
  state.selectedImages.forEach(img => URL.revokeObjectURL(img.preview));
  
  // 重置状态
  state.editorHtml = '';
  state.selectedImages = [];
  state.mentionedUsers = [];
}

/**
 * 添加 @用户
 */
export function addMentionedUser(
  chatType: 'private' | 'group',
  chatId: number,
  userId: number,
  displayName: string
): void {
  const state = getChatInputState(chatType, chatId);
  
  // 检查是否已经 @过这个用户
  const exists = state.mentionedUsers.some(u => u.userId === userId);
  if (!exists) {
    state.mentionedUsers.push({ userId, displayName });
  }
}

/**
 * 移除 @用户
 */
export function removeMentionedUser(
  chatType: 'private' | 'group',
  chatId: number,
  userId: number
): void {
  const state = getChatInputState(chatType, chatId);
  const index = state.mentionedUsers.findIndex(u => u.userId === userId);
  if (index !== -1) {
    state.mentionedUsers.splice(index, 1);
  }
}

/**
 * 获取所有聊天的输入状态（用于调试）
 */
export function getAllChatInputStates(): Map<string, ChatInputState> {
  return chatInputStates;
}

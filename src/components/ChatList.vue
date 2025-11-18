<script setup lang="ts">
import { ref, onMounted, watch } from 'vue';
import { getMessages } from '../services/storage';
import { parseCQCode } from '../utils/cqcode';
import { getFaceDisplayText } from '../utils/qq-face';
import { getContactName, getGroupName, useContactsState } from '../stores/contacts';

const props = defineProps<{
  selfId?: number;
}>();

const emit = defineEmits<{
  selectChat: [chat: ChatItem]
}>();

interface ChatItem {
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

const chats = ref<ChatItem[]>([]);
const selectedChatId = ref<string | null>(null);

// 使用全局联系人列表和群组列表
const contactsState = useContactsState();

// 格式化消息预览（将图片和表情替换为文本）
const formatMessagePreview = (message: string): string => {
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
};

// 从消息中提取聊天列表
const updateChatList = async () => {
  // 如果没有 selfId，清空列表（等待连接）
  if (!props.selfId) {
    chats.value = [];
    return;
  }

  try {
    // 获取最近的消息（按用户/群组分组）
    // 优化：只获取最近的消息，减少查询时间
    // 每个聊天只需要最新一条消息来显示在列表中
    const messages = await getMessages({ 
      limit: 500, // 减少到 500 条，通常足够显示所有聊天
      selfId: props.selfId 
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
        
        // 头像将在 setChatAvatars 中统一设置
        
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
    chats.value = Array.from(chatMap.values()).sort((a, b) => {
      const timeA = a.lastTime || 0;
      const timeB = b.lastTime || 0;
      return timeB - timeA;
    });
    
    // 更新后设置头像
    setChatAvatars();
  } catch (error) {
    console.error('更新聊天列表失败:', error);
  }
};

// 设置聊天头像 URL（直接使用 asset://avatar/ 格式）
const setChatAvatars = () => {
  for (const chat of chats.value) {
    // 如果已经有头像或已经加载失败，跳过
    if (chat.avatar || chat.avatarFailed) {
      continue;
    }
    
    if (chat.type === 'private' && chat.userId) {
      chat.avatar = `asset://avatar/user/${chat.userId}.png`;
    } else if (chat.type === 'group' && chat.groupId) {
      chat.avatar = `asset://avatar/group/${chat.groupId}.png`;
    }
  }
};

// 处理头像加载错误
const handleAvatarError = (event: Event, chatId: string) => {
  const img = event.target as HTMLImageElement;
  // 找到对应的聊天并清除头像 URL，标记为失败，避免重复加载
  const chat = chats.value.find(c => c.id === chatId);
  if (chat) {
    chat.avatar = undefined;
    chat.avatarFailed = true; // 标记为失败，避免重复尝试
  }
  img.style.display = 'none';
};

const selectChat = (chat: ChatItem) => {
  selectedChatId.value = chat.id;
  emit('selectChat', chat);
};

// 监听 selfId 变化，当有值时立即更新聊天列表
watch(() => props.selfId, () => {
  // 无论 selfId 是否有值，都更新列表
  // 如果没有 selfId，会显示空列表（等待连接）
  updateChatList();
}, { immediate: true });

// 监听全局联系人列表和群组列表变化，更新聊天列表
watch(() => [contactsState.contacts, contactsState.groups], () => {
  // 只有在有 selfId 时才更新（避免不必要的查询）
  if (props.selfId) {
    updateChatList();
  }
}, { deep: true });

// 监听聊天列表变化，设置头像
watch(() => chats.value, () => {
  setChatAvatars();
}, { deep: true });

// 监听 selfId 变化，重新设置头像
watch(() => props.selfId, () => {
  if (props.selfId) {
    setChatAvatars();
  }
});

onMounted(() => {
  // 立即尝试更新（如果没有 selfId，会显示空列表）
  updateChatList();
});

// 暴露更新方法供父组件调用
defineExpose({
  updateChatList,
});
</script>

<template>
  <div class="chat-list">
    <div class="list-content">
      <div
        v-for="chat in chats"
        :key="chat.id"
        class="chat-item"
        :class="{ active: selectedChatId === chat.id }"
        @click="selectChat(chat)"
      >
        <div class="chat-avatar">
          <img 
            v-if="chat.avatar" 
            :src="chat.avatar" 
            :alt="chat.name"
            class="avatar-image"
            @error="(e) => handleAvatarError(e, chat.id)"
          />
          <div v-else class="avatar-placeholder">
            {{ chat.name.charAt(0) }}
          </div>
        </div>
        <div class="chat-info">
          <div class="chat-header">
            <span class="chat-name">{{ chat.name }}</span>
            <span v-if="chat.lastTime" class="chat-time">
              {{ formatTime(chat.lastTime) }}
            </span>
          </div>
          <div class="chat-preview">
            <span class="preview-text">{{ chat.lastMessage || '暂无消息' }}</span>
            <span v-if="chat.unreadCount > 0" class="unread-badge">
              {{ chat.unreadCount > 99 ? '99+' : chat.unreadCount }}
            </span>
          </div>
        </div>
      </div>
      <div v-if="chats.length === 0" class="empty-state">
        暂无聊天记录
      </div>
    </div>
  </div>
</template>

<script lang="ts">
export default {
  methods: {
    formatTime(timestamp: number): string {
      const date = new Date(timestamp * 1000);
      const now = new Date();
      const diff = now.getTime() - date.getTime();
      const days = Math.floor(diff / (1000 * 60 * 60 * 24));

      if (days === 0) {
        return date.toLocaleTimeString('zh-CN', { hour: '2-digit', minute: '2-digit' });
      } else if (days === 1) {
        return '昨天';
      } else if (days < 7) {
        return `${days}天前`;
      } else {
        return date.toLocaleDateString('zh-CN', { month: 'short', day: 'numeric' });
      }
    },
  },
};
</script>

<style scoped>
.chat-list {
  display: flex;
  flex-direction: column;
  height: 100%;
  background: white;
}

.list-content {
  flex: 1;
  overflow-y: auto;
}

.chat-item {
  display: flex;
  padding: 12px 16px;
  cursor: pointer;
  transition: background-color 0.2s;
  border-bottom: 1px solid #f0f0f0;
}

.chat-item:hover {
  background-color: #f5f5f5;
}

.chat-item.active {
  background-color: #e3f2fd;
  border-left: 3px solid #2196f3;
}

.chat-avatar {
  margin-right: 12px;
  flex-shrink: 0;
}

.avatar-placeholder {
  width: 48px;
  height: 48px;
  border-radius: 50%;
  background: linear-gradient(135deg, #667eea 0%, #764ba2 100%);
  color: white;
  display: flex;
  align-items: center;
  justify-content: center;
  font-size: 18px;
  font-weight: 600;
}

.avatar-image {
  width: 48px;
  height: 48px;
  border-radius: 50%;
  object-fit: cover;
  background: #f0f0f0;
}

.chat-info {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  justify-content: center;
}

.chat-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 4px;
}

.chat-name {
  font-size: 15px;
  font-weight: 500;
  color: #333;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.chat-time {
  font-size: 12px;
  color: #999;
  flex-shrink: 0;
  margin-left: 8px;
}

.chat-preview {
  display: flex;
  justify-content: space-between;
  align-items: center;
}

.preview-text {
  font-size: 13px;
  color: #666;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  flex: 1;
}

.unread-badge {
  background-color: #f44336;
  color: white;
  font-size: 11px;
  font-weight: 600;
  padding: 2px 6px;
  border-radius: 10px;
  margin-left: 8px;
  flex-shrink: 0;
  min-width: 18px;
  text-align: center;
}

.empty-state {
  padding: 40px 20px;
  text-align: center;
  color: #999;
  font-size: 14px;
}
</style>


<script setup lang="ts">
import { ref, computed, onMounted, watch } from 'vue';
import { getChatsState, updateChatList, setChatAvatarFailed, type ChatItem } from '../stores/chats';
import { useContactsState } from '../stores/contacts';

const props = defineProps<{
  selfId?: number;
}>();

const emit = defineEmits<{
  selectChat: [chat: ChatItem]
}>();

const selectedChatId = ref<string | null>(null);

// 使用全局对话列表状态
// 直接获取 state 对象，确保响应式追踪正确
const chatsState = getChatsState();
// 直接使用 state.chats，确保响应式追踪
const chats = computed(() => chatsState.chats);

// 使用全局联系人列表和群组列表（用于触发更新）
const contactsState = useContactsState();

// 更新聊天列表（调用全局 store 的方法）
const refreshChatList = async () => {
  await updateChatList(props.selfId);
};

// 处理头像加载错误
const handleAvatarError = (event: Event, chatId: string) => {
  const img = event.target as HTMLImageElement;
  // 使用 store 方法更新头像加载失败状态
  setChatAvatarFailed(chatId, true);
  img.style.display = 'none';
};

const selectChat = (chat: ChatItem) => {
  selectedChatId.value = chat.id;
  emit('selectChat', chat);
};

// 监听全局对话列表状态变化（确保组件能响应全局状态的更新）
// 使用 immediate: true 确保在初始化时也能触发，这样即使全局状态在组件挂载前已更新，也能正确显示
watch(() => chatsState.chats, (newChats, oldChats) => {
  console.log('[ChatList] 全局对话列表状态已更新，当前对话数:', newChats.length, '之前:', oldChats?.length || 0);
  // 这里不需要做任何操作，因为 chats computed 会自动更新
  // 但添加这个 watch 可以确保响应式系统知道需要追踪这个变化
}, { deep: true, immediate: true });

// 监听 selfId 变化，当有值时立即更新聊天列表
watch(() => props.selfId, async (newSelfId, oldSelfId) => {
  // 如果 selfId 从无到有，或者发生了变化，更新列表
  if (newSelfId && newSelfId !== oldSelfId) {
    console.log('[ChatList] selfId 变化，更新聊天列表:', newSelfId);
    await refreshChatList();
  } else if (!newSelfId) {
    // 如果没有 selfId，清空列表
    console.log('[ChatList] 没有 selfId，清空列表');
    const { getChatsState } = await import('../stores/chats');
    getChatsState().chats = [];
  }
}, { immediate: true });

// 监听全局联系人列表和群组列表变化，更新聊天列表
watch(() => [contactsState.contacts, contactsState.groups], () => {
  // 只有在有 selfId 时才更新（避免不必要的查询）
  if (props.selfId) {
    console.log('[ChatList] 联系人/群组列表变化，更新聊天列表');
    refreshChatList();
  }
}, { deep: true });

onMounted(async () => {
  // 组件挂载时，如果全局状态已经有数据，直接使用（不需要重新加载）
  // 如果全局状态为空且有 selfId，则主动加载
  if (props.selfId) {
    if (chatsState.chats.length === 0) {
      console.log('[ChatList] 组件挂载，有 selfId 但全局状态为空，主动加载聊天列表:', props.selfId);
      await refreshChatList();
    } else {
      console.log('[ChatList] 组件挂载，全局状态已有数据，直接使用，对话数:', chatsState.chats.length);
    }
  } else {
    console.log('[ChatList] 组件挂载，没有 selfId，等待连接');
  }
});

// 暴露更新方法供父组件调用
defineExpose({
  updateChatList: refreshChatList,
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
  padding: 12px 12px;
  cursor: pointer;
  transition: background-color 0.15s;
  border-bottom: none;
  position: relative;
}

.chat-item::after {
  content: '';
  position: absolute;
  bottom: 0;
  left: 76px;
  right: 12px;
  height: 1px;
  background: #f0f0f0;
}

.chat-item:last-child::after {
  display: none;
}

.chat-item:hover {
  background-color: #f4f4f5;
}

.chat-item.active {
  background-color: #e7f2ff;
}

.chat-item.active::before {
  content: '';
  position: absolute;
  left: 0;
  top: 0;
  bottom: 0;
  width: 3px;
  background: #0088cc;
}

.chat-avatar {
  margin-right: 12px;
  flex-shrink: 0;
}

.avatar-placeholder {
  width: 52px;
  height: 52px;
  border-radius: 50%;
  background: linear-gradient(135deg, #0088cc 0%, #006ba6 100%);
  color: white;
  display: flex;
  align-items: center;
  justify-content: center;
  font-size: 20px;
  font-weight: 500;
}

.avatar-image {
  width: 52px;
  height: 52px;
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
  color: #222;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.chat-time {
  font-size: 12px;
  color: #8e8e93;
  flex-shrink: 0;
  margin-left: 8px;
}

.chat-preview {
  display: flex;
  justify-content: space-between;
  align-items: center;
}

.preview-text {
  font-size: 14px;
  color: #8e8e93;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  flex: 1;
}

.unread-badge {
  background: #0088cc;
  color: white;
  font-size: 12px;
  font-weight: 600;
  min-width: 20px;
  height: 20px;
  border-radius: 10px;
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 0 6px;
  margin-left: 8px;
  flex-shrink: 0;
}

.empty-state {
  padding: 60px 20px;
  text-align: center;
  color: #8e8e93;
  font-size: 14px;
}
</style>


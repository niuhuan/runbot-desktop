<script setup lang="ts">
import { ref, watch, nextTick, onMounted } from 'vue';
import { runbotService } from '../services/runbot';
import { useContactsState, updateContacts } from '../stores/contacts';

const props = defineProps<{
  selfId?: number;
}>();

const emit = defineEmits<{
  selectContact: [contact: ContactItem]
}>();

interface ContactItem {
  userId: number;
  nickname: string;
  remark?: string;
  avatar?: string;
  avatarFailed?: boolean; // 标记头像是否加载失败
}

const contacts = ref<ContactItem[]>([]);
const loading = ref(false);
const selectedUserId = ref<number | null>(null);

// 使用全局联系人列表
const contactsState = useContactsState();

// 监听全局联系人列表变化，同步到本地
watch(() => contactsState.contacts, (newContacts) => {
  try {
    if (!newContacts || !Array.isArray(newContacts)) {
      return;
    }
    contacts.value = newContacts.map(c => ({
      userId: c.userId,
      nickname: c.nickname,
      remark: c.remark,
      avatar: undefined,
      avatarFailed: false,
    }));
    // 使用 nextTick 确保 DOM 更新后再设置头像
    nextTick(() => {
      try {
        setContactAvatars();
      } catch (error) {
        console.error('设置联系人头像失败:', error);
      }
    });
  } catch (error) {
    console.error('更新联系人列表失败:', error);
  }
}, { immediate: false, deep: true });

// 获取联系人列表
const loadContacts = async () => {
  loading.value = true;
  try {
    // 调用 OneBot API 获取好友列表
    // 数据会通过消息事件返回，在 MainView 中更新全局 store
    await runbotService.getFriendList();
  } catch (error) {
    console.error('获取联系人列表失败:', error);
  } finally {
    loading.value = false;
  }
};

const selectContact = (contact: ContactItem) => {
  selectedUserId.value = contact.userId;
  emit('selectContact', contact);
};

// 组件挂载后，初始化数据（从全局 store 读取）
onMounted(() => {
  try {
    if (contactsState.contacts && Array.isArray(contactsState.contacts)) {
      contacts.value = contactsState.contacts.map(c => ({
        userId: c.userId,
        nickname: c.nickname,
        remark: c.remark,
        avatar: undefined,
        avatarFailed: false,
      }));
      nextTick(() => {
        setContactAvatars();
      });
    }
  } catch (error) {
    console.error('初始化联系人列表失败:', error);
  }
});

// 设置联系人头像 URL（直接使用 asset://avatar/ 格式）
const setContactAvatars = () => {
  try {
    for (const contact of contacts.value) {
      // 如果已经有头像或已经加载失败，跳过
      if (contact.avatar || contact.avatarFailed) {
        continue;
      }
      
      contact.avatar = `asset://avatar/user/${contact.userId}.png`;
    }
  } catch (error) {
    console.error('设置联系人头像失败:', error);
  }
};

// 处理头像加载错误
const handleAvatarError = (event: Event, userId: number) => {
  const img = event.target as HTMLImageElement;
  // 找到对应的联系人并清除头像 URL，标记为失败，避免重复加载
  const contact = contacts.value.find(c => c.userId === userId);
  if (contact) {
    contact.avatar = undefined;
    contact.avatarFailed = true; // 标记为失败，避免重复尝试
  }
  img.style.display = 'none';
};

// 监听 selfId 变化，重新设置头像
watch(() => props.selfId, () => {
  if (props.selfId) {
    nextTick(() => {
      try {
        setContactAvatars();
      } catch (error) {
        console.error('设置联系人头像失败:', error);
      }
    });
  }
});

// 暴露更新方法
defineExpose({
  loadContacts,
  updateContacts: (newContacts: ContactItem[]) => {
    // 更新全局 store
    updateContacts(newContacts.map(c => ({
      userId: c.userId,
      nickname: c.nickname,
      remark: c.remark,
    })));
    // 本地数据会通过 watch 自动同步
  },
});
</script>

<template>
  <div class="contact-list">
    <div class="list-content">
      <div v-if="loading" class="loading-state">
        加载中...
      </div>
      <div
        v-for="contact in contacts"
        :key="contact.userId"
        class="contact-item"
        :class="{ active: selectedUserId === contact.userId }"
        @click="selectContact(contact)"
      >
        <div class="contact-avatar">
          <img 
            v-if="contact.avatar" 
            :src="contact.avatar" 
            :alt="contact.remark || contact.nickname"
            class="avatar-image"
            @error="(e) => handleAvatarError(e, contact.userId)"
          />
          <div v-else class="avatar-placeholder">
            {{ (contact.remark || contact.nickname).charAt(0) }}
          </div>
        </div>
        <div class="contact-info">
          <div class="contact-name">
            {{ contact.remark || contact.nickname }}
          </div>
          <div v-if="contact.remark && contact.remark !== contact.nickname" class="contact-nickname">
            {{ contact.nickname }}
          </div>
        </div>
      </div>
      <div v-if="!loading && contacts.length === 0" class="empty-state">
        暂无联系人
        <button @click="loadContacts" class="reload-btn">重新加载</button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.contact-list {
  display: flex;
  flex-direction: column;
  height: 100%;
  background: white;
  border-right: 1px solid #e0e0e0;
}

.list-content {
  flex: 1;
  overflow-y: auto;
}

.contact-item {
  display: flex;
  padding: 12px 16px;
  cursor: pointer;
  transition: background-color 0.2s;
  border-bottom: 1px solid #f0f0f0;
}

.contact-item:hover {
  background-color: #f5f5f5;
}

.contact-item.active {
  background-color: #e3f2fd;
  border-left: 3px solid #2196f3;
}

.contact-avatar {
  margin-right: 12px;
  flex-shrink: 0;
}

.avatar-image {
  width: 48px;
  height: 48px;
  border-radius: 50%;
  object-fit: cover;
}

.avatar-placeholder {
  width: 48px;
  height: 48px;
  border-radius: 50%;
  background: linear-gradient(135deg, #4caf50 0%, #45a049 100%);
  color: white;
  display: flex;
  align-items: center;
  justify-content: center;
  font-size: 18px;
  font-weight: 600;
}

.contact-info {
  flex: 1;
  display: flex;
  flex-direction: column;
  justify-content: center;
}

.contact-name {
  font-size: 15px;
  font-weight: 500;
  color: #333;
  margin-bottom: 2px;
}

.contact-nickname {
  font-size: 12px;
  color: #999;
}

.loading-state,
.empty-state {
  padding: 40px 20px;
  text-align: center;
  color: #999;
  font-size: 14px;
}

.reload-btn {
  margin-top: 12px;
  padding: 8px 16px;
  background-color: #2196f3;
  color: white;
  border: none;
  border-radius: 4px;
  cursor: pointer;
  font-size: 14px;
}

.reload-btn:hover {
  background-color: #1976d2;
}
</style>


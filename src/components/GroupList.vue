<script setup lang="ts">
import { ref, watch, nextTick, onMounted } from 'vue';
import { runbotService } from '../services/runbot';
import { useContactsState, updateGroups } from '../stores/contacts';

const emit = defineEmits<{
  selectGroup: [group: GroupItem]
}>();

interface GroupItem {
  groupId: number;
  groupName: string;
  memberCount?: number;
  avatar?: string;
  avatarFailed?: boolean;
}

const groups = ref<GroupItem[]>([]);
const loading = ref(false);
const selectedGroupId = ref<number | null>(null);

// 使用全局群组列表
const contactsState = useContactsState();

// 监听全局群组列表变化，同步到本地
watch(() => contactsState.groups, (newGroups) => {
  try {
    if (!newGroups || !Array.isArray(newGroups)) {
      return;
    }
    groups.value = newGroups.map(g => ({
      groupId: g.groupId,
      groupName: g.groupName,
      memberCount: g.memberCount,
      avatar: undefined,
      avatarFailed: false,
    }));
    // 使用 nextTick 确保 DOM 更新后再设置头像
    nextTick(() => {
      try {
        setGroupAvatars();
      } catch (error) {
        console.error('设置群组头像失败:', error);
      }
    });
  } catch (error) {
    console.error('更新群组列表失败:', error);
  }
}, { immediate: false, deep: true });

// 获取群组列表
const loadGroups = async () => {
  loading.value = true;
  try {
    // 调用 OneBot API 获取群组列表
    // 数据会通过消息事件返回，在 MainView 中更新全局 store
    await runbotService.getGroupList();
  } catch (error) {
    console.error('获取群组列表失败:', error);
  } finally {
    loading.value = false;
  }
};

const props = defineProps<{
  selfId?: number;
}>();

// 设置群组头像 URL（直接使用 asset://avatar/ 格式）
const setGroupAvatars = () => {
  try {
    for (const group of groups.value) {
      if (group.avatar || group.avatarFailed) continue;
      
      group.avatar = `asset://avatar/group/${group.groupId}.png`;
    }
  } catch (error) {
    console.error('设置群组头像失败:', error);
  }
};

// 处理头像加载错误
const handleAvatarError = (event: Event, groupId: number) => {
  const img = event.target as HTMLImageElement;
  const group = groups.value.find(g => g.groupId === groupId);
  if (group) {
    group.avatar = undefined;
    group.avatarFailed = true;
  }
  img.style.display = 'none';
};

// 监听群组列表变化，设置头像（移除这个 watch，因为已经在全局列表变化时设置了）
// watch(() => groups.value, () => {
//   setGroupAvatars();
// }, { deep: true });

// 监听 selfId 变化，重新设置头像
watch(() => props.selfId, () => {
  if (props.selfId) {
    nextTick(() => {
      try {
        setGroupAvatars();
      } catch (error) {
        console.error('设置群组头像失败:', error);
      }
    });
  }
});

const selectGroup = (group: GroupItem) => {
  selectedGroupId.value = group.groupId;
  emit('selectGroup', group);
};

// 组件挂载后，初始化数据（从全局 store 读取）
onMounted(() => {
  try {
    if (contactsState.groups && Array.isArray(contactsState.groups)) {
      groups.value = contactsState.groups.map(g => ({
        groupId: g.groupId,
        groupName: g.groupName,
        memberCount: g.memberCount,
        avatar: undefined,
        avatarFailed: false,
      }));
      nextTick(() => {
        setGroupAvatars();
      });
    }
  } catch (error) {
    console.error('初始化群组列表失败:', error);
  }
});

// 暴露更新方法
defineExpose({
  loadGroups,
  updateGroups: (newGroups: GroupItem[]) => {
    // 更新全局 store
    updateGroups(newGroups.map(g => ({
      groupId: g.groupId,
      groupName: g.groupName,
      memberCount: g.memberCount,
    })));
    // 本地数据会通过 watch 自动同步
  },
});
</script>

<template>
  <div class="group-list">
    <div class="list-content">
      <div v-if="loading" class="loading-state">
        加载中...
      </div>
      <div
        v-for="group in groups"
        :key="group.groupId"
        class="group-item"
        :class="{ active: selectedGroupId === group.groupId }"
        @click="selectGroup(group)"
      >
        <div class="group-avatar">
          <img 
            v-if="group.avatar" 
            :src="group.avatar" 
            :alt="group.groupName"
            class="avatar-image"
            @error="(e) => handleAvatarError(e, group.groupId)"
          />
          <div v-else class="avatar-placeholder">
            {{ group.groupName.charAt(0) }}
          </div>
        </div>
        <div class="group-info">
          <div class="group-name">
            {{ group.groupName }}
          </div>
          <div v-if="group.memberCount" class="group-meta">
            {{ group.memberCount }} 成员
          </div>
        </div>
      </div>
      <div v-if="!loading && groups.length === 0" class="empty-state">
        暂无群组
        <button @click="loadGroups" class="reload-btn">重新加载</button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.group-list {
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

.group-item {
  display: flex;
  padding: 12px 16px;
  cursor: pointer;
  transition: background-color 0.2s;
  border-bottom: 1px solid #f0f0f0;
}

.group-item:hover {
  background-color: #f5f5f5;
}

.group-item.active {
  background-color: #e3f2fd;
  border-left: 3px solid #2196f3;
}

.group-avatar {
  margin-right: 12px;
  flex-shrink: 0;
}

.avatar-placeholder {
  width: 48px;
  height: 48px;
  border-radius: 50%;
  background: linear-gradient(135deg, #ff9800 0%, #f57c00 100%);
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

.group-info {
  flex: 1;
  display: flex;
  flex-direction: column;
  justify-content: center;
}

.group-name {
  font-size: 15px;
  font-weight: 500;
  color: #333;
  margin-bottom: 2px;
}

.group-meta {
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


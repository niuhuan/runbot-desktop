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
}

.list-content {
  flex: 1;
  overflow-y: auto;
}

.group-item {
  position: relative;
  display: flex;
  padding: 12px 16px;
  cursor: pointer;
  transition: background-color 0.15s;
}

.group-item::after {
  content: '';
  position: absolute;
  left: 76px;
  right: 16px;
  bottom: 0;
  height: 1px;
  background-color: #e8e8e8;
}

.group-item:last-child::after {
  display: none;
}

.group-item:hover {
  background-color: #f4f4f5;
}

.group-item.active {
  background-color: #e7f2ff;
}

.group-item.active::before {
  content: '';
  position: absolute;
  left: 0;
  top: 0;
  bottom: 0;
  width: 3px;
  background-color: #0088cc;
}

.group-avatar {
  margin-right: 12px;
  flex-shrink: 0;
}

.avatar-placeholder {
  width: 52px;
  height: 52px;
  border-radius: 50%;
  background: linear-gradient(135deg, #0088cc 0%, #006699 100%);
  color: white;
  display: flex;
  align-items: center;
  justify-content: center;
  font-size: 20px;
  font-weight: 600;
  box-shadow: 0 1px 3px rgba(0, 0, 0, 0.1);
}

.avatar-image {
  width: 52px;
  height: 52px;
  border-radius: 50%;
  object-fit: cover;
  background: #f4f4f5;
}

.group-info {
  flex: 1;
  display: flex;
  flex-direction: column;
  justify-content: center;
  min-width: 0;
}

.group-name {
  font-size: 15px;
  font-weight: 500;
  color: #222;
  margin-bottom: 3px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.group-meta {
  font-size: 13px;
  color: #8e8e93;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.loading-state,
.empty-state {
  padding: 40px 20px;
  text-align: center;
  color: #8e8e93;
  font-size: 14px;
}

.reload-btn {
  margin-top: 12px;
  padding: 8px 20px;
  background-color: #0088cc;
  color: white;
  border: none;
  border-radius: 8px;
  cursor: pointer;
  font-size: 14px;
  font-weight: 500;
  transition: background-color 0.15s;
}

.reload-btn:hover {
  background-color: #006699;
}

.reload-btn:active {
  background-color: #005580;
}
</style>


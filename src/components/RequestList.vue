<script setup lang="ts">
import { ref, computed, watch } from 'vue';
import { runbotService } from '../services/runbot';
import { useRequestsStore, type RequestItem } from '../stores/requests';

const requestsStore = useRequestsStore();
const selectedTab = ref<'pending' | 'history'>('pending');

// 获取用户头像URL
const getUserAvatarUrl = (userId: number) => {
  return `asset://avatar/user/${userId}.png`;
};

// 待处理的请求
const pendingRequests = computed(() => {
  return requestsStore.getPendingRequests();
});

// 历史记录
const historyRequests = computed(() => {
  return requestsStore.getHistoryRequests();
});

// 显示的请求列表
const displayRequests = computed(() => {
  return selectedTab.value === 'pending' ? pendingRequests.value : historyRequests.value;
});

// 监听切换到待处理标签,自动标记所有未读请求为已读
watch(selectedTab, (newTab) => {
  if (newTab === 'pending') {
    const unreadRequests = pendingRequests.value.filter(r => !r.is_read);
    unreadRequests.forEach(request => {
      requestsStore.markAsRead(request.flag);
    });
  }
});

// 格式化时间
const formatTime = (timestamp: number) => {
  const date = new Date(timestamp * 1000);
  const now = new Date();
  const diffMs = now.getTime() - date.getTime();
  const diffMins = Math.floor(diffMs / 60000);
  const diffHours = Math.floor(diffMs / 3600000);
  const diffDays = Math.floor(diffMs / 86400000);

  if (diffMins < 1) return '刚刚';
  if (diffMins < 60) return `${diffMins}分钟前`;
  if (diffHours < 24) return `${diffHours}小时前`;
  if (diffDays < 7) return `${diffDays}天前`;
  
  return date.toLocaleDateString('zh-CN', {
    year: 'numeric',
    month: '2-digit',
    day: '2-digit',
    hour: '2-digit',
    minute: '2-digit',
  });
};

// 获取请求类型文本
const getRequestTypeText = (item: RequestItem) => {
  if (item.request_type === 'friend') {
    return '好友请求';
  } else if (item.sub_type === 'add') {
    return '申请加群';
  } else if (item.sub_type === 'invite') {
    return '邀请加群';
  }
  return '群组请求';
};

// 获取请求描述
const getRequestDescription = (item: RequestItem) => {
  if (item.request_type === 'friend') {
    return `${item.nickname || item.user_id} 请求添加你为好友`;
  } else if (item.sub_type === 'add') {
    return `${item.nickname || item.user_id} 申请加入群 ${item.group_name || item.group_id}`;
  } else if (item.sub_type === 'invite') {
    return `${item.nickname || item.user_id} 邀请你加入群 ${item.group_name || item.group_id}`;
  }
  return '未知请求';
};

// 同意请求
const approveRequest = async (item: RequestItem) => {
  try {
    if (item.request_type === 'friend') {
      // 处理好友请求
      await runbotService.setFriendAddRequest(item.flag, true, item.comment || '');
    } else {
      // 处理群组请求
      await runbotService.setGroupAddRequest(
        item.flag,
        item.sub_type || 'add',
        true,
        ''
      );
    }
    
    // 更新状态
    requestsStore.updateRequestStatus(item.flag, 'approved');
    console.log('已同意请求:', item);
  } catch (error) {
    console.error('同意请求失败:', error);
    alert('操作失败，请重试');
  }
};

// 拒绝请求
const rejectRequest = async (item: RequestItem) => {
  try {
    if (item.request_type === 'friend') {
      // 处理好友请求
      await runbotService.setFriendAddRequest(item.flag, false, '');
    } else {
      // 处理群组请求
      await runbotService.setGroupAddRequest(
        item.flag,
        item.sub_type || 'add',
        false,
        ''
      );
    }
    
    // 更新状态
    requestsStore.updateRequestStatus(item.flag, 'rejected');
    console.log('已拒绝请求:', item);
  } catch (error) {
    console.error('拒绝请求失败:', error);
    alert('操作失败，请重试');
  }
};

// 添加请求（暴露给父组件）
const addRequest = (request: RequestItem) => {
  requestsStore.addRequest(request);
};

// 清空历史记录
const clearHistory = () => {
  if (confirm('确定要清空历史记录吗？')) {
    requestsStore.clearHistory();
  }
};

// 暴露方法
defineExpose({
  addRequest,
});
</script>

<template>
  <div class="request-list">
    <div class="header">
      <div class="tabs">
        <button
          class="tab"
          :class="{ active: selectedTab === 'pending' }"
          @click="selectedTab = 'pending'"
        >
          待处理
          <span v-if="requestsStore.unreadCount.value > 0" class="badge">
            {{ requestsStore.unreadCount.value }}
          </span>
        </button>
        <button
          class="tab"
          :class="{ active: selectedTab === 'history' }"
          @click="selectedTab = 'history'"
        >
          历史记录
        </button>
      </div>
      <button
        v-if="selectedTab === 'history' && historyRequests.length > 0"
        class="clear-btn"
        @click="clearHistory"
      >
        清空
      </button>
    </div>

    <div class="list-content">
      <div v-if="displayRequests.length === 0" class="empty-state">
        <svg class="empty-icon" viewBox="0 0 24 24" fill="currentColor">
          <path d="M20 6h-2.18c.11-.31.18-.65.18-1 0-1.66-1.34-3-3-3-1.05 0-1.96.54-2.5 1.35l-.5.67-.5-.68C10.96 2.54 10.05 2 9 2 7.34 2 6 3.34 6 5c0 .35.07.69.18 1H4c-1.11 0-1.99.89-1.99 2L2 19c0 1.11.89 2 2 2h16c1.11 0 2-.89 2-2V8c0-1.11-.89-2-2-2zm-5-2c.55 0 1 .45 1 1s-.45 1-1 1-1-.45-1-1 .45-1 1-1zM9 4c.55 0 1 .45 1 1s-.45 1-1 1-1-.45-1-1 .45-1 1-1zm11 15H4v-2h16v2zm0-5H4V8h5.08L7 10.83 8.62 12 11 8.76l1-1.36 1 1.36L15.38 12 17 10.83 14.92 8H20v6z"/>
        </svg>
        <div class="empty-text">
          {{ selectedTab === 'pending' ? '暂无待处理请求' : '暂无历史记录' }}
        </div>
      </div>

      <div
        v-for="request in displayRequests"
        :key="request.id"
        class="request-item"
        :class="{ [request.status]: true, unread: !request.is_read && request.status === 'pending' }"
      >
        <div class="request-avatar">
          <img 
            :src="getUserAvatarUrl(request.user_id)" 
            class="avatar-image"
            :alt="request.nickname || String(request.user_id)"
          />
          <div v-if="!request.is_read && request.status === 'pending'" class="unread-indicator"></div>
        </div>

        <div class="request-content">
          <div class="request-header">
            <span class="request-type">{{ getRequestTypeText(request) }}</span>
            <span class="request-time">{{ formatTime(request.time) }}</span>
          </div>
          <div class="request-description">
            {{ getRequestDescription(request) }}
          </div>
          <div v-if="request.comment" class="request-comment">
            <span class="comment-label">验证消息：</span>
            <span class="comment-text">{{ request.comment }}</span>
          </div>

          <!-- 待处理状态显示操作按钮 -->
          <div v-if="request.status === 'pending'" class="request-actions">
            <button class="btn-approve" @click="approveRequest(request)">
              同意
            </button>
            <button class="btn-reject" @click="rejectRequest(request)">
              拒绝
            </button>
          </div>

          <!-- 已处理状态显示结果 -->
          <div v-else class="request-status">
            <span v-if="request.status === 'approved'" class="status-approved">
              <svg class="status-icon" viewBox="0 0 24 24" fill="currentColor">
                <path d="M9 16.17L4.83 12l-1.42 1.41L9 19 21 7l-1.41-1.41L9 16.17z"/>
              </svg>
              已同意
            </span>
            <span v-else-if="request.status === 'rejected'" class="status-rejected">
              <svg class="status-icon" viewBox="0 0 24 24" fill="currentColor">
                <path d="M19 6.41L17.59 5 12 10.59 6.41 5 5 6.41 10.59 12 5 17.59 6.41 19 12 13.41 17.59 19 19 17.59 13.41 12 19 6.41z"/>
              </svg>
              已拒绝
            </span>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.request-list {
  display: flex;
  flex-direction: column;
  height: 100%;
  background: white;
}

.header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 16px;
  border-bottom: 1px solid #e0e0e0;
  background: white;
}

.tabs {
  display: flex;
  gap: 8px;
}

.tab {
  position: relative;
  padding: 8px 16px;
  border: none;
  background: transparent;
  color: #666;
  font-size: 14px;
  cursor: pointer;
  border-radius: 4px;
  transition: all 0.2s;
}

.tab:hover {
  background: #f5f5f5;
}

.tab.active {
  color: #2196f3;
  background: #e3f2fd;
  font-weight: 500;
}

.badge {
  display: inline-block;
  margin-left: 6px;
  padding: 2px 6px;
  background: #f44336;
  color: white;
  font-size: 12px;
  border-radius: 10px;
  min-width: 18px;
  text-align: center;
}

.clear-btn {
  padding: 6px 12px;
  border: 1px solid #ddd;
  background: white;
  color: #666;
  font-size: 13px;
  cursor: pointer;
  border-radius: 4px;
  transition: all 0.2s;
}

.clear-btn:hover {
  background: #f5f5f5;
  color: #333;
}

.list-content {
  flex: 1;
  overflow-y: auto;
  padding: 8px;
}

.empty-state {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  height: 100%;
  color: #999;
}

.empty-icon {
  width: 64px;
  height: 64px;
  margin-bottom: 16px;
  opacity: 0.5;
}

.empty-text {
  font-size: 14px;
}

.request-item {
  display: flex;
  gap: 12px;
  padding: 16px;
  margin-bottom: 8px;
  background: white;
  border: 1px solid #e0e0e0;
  border-radius: 8px;
  transition: all 0.2s;
}

.request-item:hover {
  box-shadow: 0 2px 8px rgba(0, 0, 0, 0.1);
}

.request-item.pending {
  border-left: 3px solid #ff9800;
}

.request-item.approved {
  opacity: 0.7;
  border-left: 3px solid #4caf50;
}

.request-item.rejected {
  opacity: 0.7;
  border-left: 3px solid #f44336;
}

.request-item.unread {
  background: #fafafa;
  box-shadow: 0 0 0 2px #2196f3 inset;
}

.request-avatar {
  flex-shrink: 0;
  position: relative;
}

.avatar-image {
  width: 48px;
  height: 48px;
  border-radius: 50%;
  object-fit: cover;
  background: #f0f0f0;
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
  font-size: 24px;
}

.unread-indicator {
  position: absolute;
  top: 0;
  right: 0;
  width: 12px;
  height: 12px;
  background: #f44336;
  border: 2px solid white;
  border-radius: 50%;
}

.request-content {
  flex: 1;
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.request-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
}

.request-type {
  font-size: 13px;
  font-weight: 500;
  color: #2196f3;
}

.request-time {
  font-size: 12px;
  color: #999;
}

.request-description {
  font-size: 14px;
  color: #333;
  line-height: 1.5;
}

.request-comment {
  padding: 8px;
  background: #f5f5f5;
  border-radius: 4px;
  font-size: 13px;
  line-height: 1.5;
}

.comment-label {
  color: #666;
  font-weight: 500;
}

.comment-text {
  color: #333;
}

.request-actions {
  display: flex;
  gap: 8px;
  margin-top: 4px;
}

.request-actions button {
  flex: 1;
  padding: 8px 16px;
  border: none;
  border-radius: 4px;
  font-size: 13px;
  cursor: pointer;
  transition: all 0.2s;
}

.btn-approve {
  background: #4caf50;
  color: white;
}

.btn-approve:hover {
  background: #45a049;
}

.btn-reject {
  background: #f44336;
  color: white;
}

.btn-reject:hover {
  background: #da190b;
}

.request-status {
  margin-top: 4px;
  font-size: 13px;
  font-weight: 500;
}

.status-approved {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  color: #4caf50;
}

.status-rejected {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  color: #f44336;
}

.status-icon {
  width: 18px;
  height: 18px;
}
</style>

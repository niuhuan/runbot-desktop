import { ref, computed } from 'vue';
import { 
  updateRequestStatus as updateRequestStatusDb, 
  getRequests as getRequestsDb, 
  clearHistoryRequests as clearHistoryRequestsDb,
  markRequestRead as markRequestReadDb,
  getUnreadRequestCount as getUnreadRequestCountDb
} from '../services/request-storage';
import { getConnectionState } from './connection';

export interface RequestItem {
  id: string;
  time: number;
  request_type: 'friend' | 'group';
  sub_type: string;
  user_id: number;
  user_name?: string;
  nickname?: string;
  comment?: string;
  flag: string;
  group_id?: number;
  group_name?: string;
  status: 'pending' | 'approved' | 'rejected' | 'ignored';
  is_read?: boolean;
}

const requests = ref<RequestItem[]>([]);

export const useRequestsStore = () => {
  // 使用全局的 selfId
  const getSelfId = () => {
    const connectionState = getConnectionState();
    return connectionState.selfId || undefined;
  };

  const addRequest = (request: RequestItem) => {
    // 仅用于内存操作，实际数据由后端保存
    const exists = requests.value.some(r => r.flag === request.flag);
    if (!exists) {
      requests.value.unshift(request);
    }
  };

  const updateRequestStatus = async (flag: string, status: 'approved' | 'rejected') => {
    const request = requests.value.find(r => r.flag === flag);
    if (request) {
      request.status = status;
      // 更新数据库
      try {
        await updateRequestStatusDb(flag, status, getSelfId());
      } catch (error) {
        console.error('更新请求状态到数据库失败:', error);
      }
    }
  };

  const getPendingRequests = () => {
    return requests.value.filter(r => r.status === 'pending');
  };

  const getHistoryRequests = () => {
    return requests.value.filter(r => r.status !== 'pending');
  };

  const getAllRequests = () => {
    return requests.value;
  };

  const clearHistory = async () => {
    requests.value = requests.value.filter(r => r.status === 'pending');
    // 清空数据库中的历史记录
    try {
      await clearHistoryRequestsDb(getSelfId());
    } catch (error) {
      console.error('清空历史请求失败:', error);
    }
  };

  // 从数据库加载请求
  const loadRequests = async () => {
    const selfId = getSelfId();
    console.log('[RequestsStore] loadRequests 被调用, selfId:', selfId);
    try {
      const loadedRequests = await getRequestsDb(undefined, undefined, undefined, selfId);
      requests.value = loadedRequests;
      console.log(`[RequestsStore] 已从数据库加载 ${loadedRequests.length} 个请求`, loadedRequests);
    } catch (error) {
      console.error('[RequestsStore] 加载请求失败:', error);
    }
  };

  // 更新请求的显示名称（前端补充）
  const updateRequestNames = (getContactName: (userId: number) => string, getGroupName: (groupId: number) => string) => {
    requests.value.forEach(request => {
      if (request.user_id) {
        const userName = getContactName(request.user_id);
        if (userName !== `用户 ${request.user_id}`) {
          request.user_name = userName;
          request.nickname = userName;
        }
      }
      if (request.group_id) {
        const groupName = getGroupName(request.group_id);
        if (groupName !== `群 ${request.group_id}`) {
          request.group_name = groupName;
        }
      }
    });
  };

  // 标记请求为已读
  const markAsRead = async (flag: string) => {
    const request = requests.value.find(r => r.flag === flag);
    if (request && !request.is_read) {
      request.is_read = true;
      try {
        await markRequestReadDb(flag, getSelfId());
      } catch (error) {
        console.error('标记请求为已读失败:', error);
      }
    }
  };

  // 获取未读请求数量
  const unreadCount = computed(() => {
    return requests.value.filter(r => r.status === 'pending' && !r.is_read).length;
  });

  // 从数据库获取未读数量
  const getUnreadCount = async (): Promise<number> => {
    try {
      return await getUnreadRequestCountDb(getSelfId());
    } catch (error) {
      console.error('获取未读请求数量失败:', error);
      return 0;
    }
  };

  return {
    requests,
    unreadCount,
    addRequest,
    updateRequestStatus,
    getPendingRequests,
    getHistoryRequests,
    getAllRequests,
    clearHistory,
    loadRequests,
    updateRequestNames,
    markAsRead,
    getUnreadCount,
  };
};

import { invoke } from '@tauri-apps/api/core';
import type { RequestItem } from '../stores/requests';

/**
 * 保存请求到数据库
 */
export async function saveRequest(request: RequestItem, selfId?: number): Promise<string> {
  const requestData = JSON.stringify(request);
  return await invoke<string>('save_request', {
    requestData,
    selfId: selfId || null,
  });
}

/**
 * 更新请求状态
 */
export async function updateRequestStatus(
  flag: string,
  status: 'pending' | 'approved' | 'rejected',
  selfId?: number
): Promise<void> {
  await invoke('update_request_status', {
    flag,
    status,
    selfId: selfId || null,
  });
}

/**
 * 获取请求列表
 */
export async function getRequests(
  status?: 'pending' | 'approved' | 'rejected',
  limit?: number,
  offset?: number,
  selfId?: number
): Promise<RequestItem[]> {
  console.log('[request-storage] getRequests 被调用:', {
    status: status || null,
    limit: limit || null,
    offset: offset || null,
    selfId: selfId || null,
  });
  
  const results = await invoke<string[]>('get_requests', {
    status: status || null,
    limit: limit || null,
    offset: offset || null,
    selfId: selfId || null,
  });
  
  console.log('[request-storage] 收到 invoke 返回结果，数量:', results.length);
  
  const parsed = results.map(json => JSON.parse(json) as RequestItem);
  console.log('[request-storage] 解析后的请求:', parsed);
  
  return parsed;
}

/**
 * 删除请求
 */
export async function deleteRequest(flag: string, selfId?: number): Promise<void> {
  await invoke('delete_request', {
    flag,
    selfId: selfId || null,
  });
}

/**
 * 清空历史请求(只保留待处理的)
 */
export async function clearHistoryRequests(selfId?: number): Promise<number> {
  return await invoke<number>('clear_history_requests', {
    selfId: selfId || null,
  });
}

/**
 * 标记请求为已读
 */
export async function markRequestRead(flag: string, selfId?: number): Promise<void> {
  await invoke('mark_request_read', {
    flag,
    selfId: selfId || null,
  });
}

/**
 * 获取未读请求数量
 */
export async function getUnreadRequestCount(selfId?: number): Promise<number> {
  return await invoke<number>('get_unread_request_count', {
    selfId: selfId || null,
  });
}

/**
 * 全局连接状态管理
 * 统一维护应用的连接状态，供所有组件使用
 */

import { reactive, readonly, computed } from 'vue';
import { runbotService, type ConnectionStatus } from '../services/runbot';

// 全局连接状态
const state = reactive<{
  status: ConnectionStatus;
  selfId: number | null;
  initialized: boolean;
}>({
  status: {
    status: 'disconnected',
    message: '未连接',
  },
  selfId: null,
  initialized: false,
});

/**
 * 初始化连接状态管理
 * 从后端获取当前状态，并设置监听
 */
export async function initConnectionStore(): Promise<void> {
  if (state.initialized) {
    return; // 已经初始化过，避免重复初始化
  }

  // 获取初始状态（如果未连接，这是正常的，不需要报错）
  try {
    const status = await runbotService.getConnectionStatus();
    state.status = status;
  } catch (error) {
    // 未连接时获取状态失败是正常的，不需要报错
    // 状态已经在初始化时设置为 disconnected
  }

  // 获取初始 self_id（如果未连接，这是正常的，不需要报错）
  try {
    const selfId = await runbotService.getSelfId();
    state.selfId = selfId;
  } catch (error) {
    // 未连接时获取 self_id 失败是正常的，不需要报错
    // selfId 已经在初始化时设置为 null
  }

  // 监听状态变化
  await runbotService.onStatusChange((status) => {
    state.status = status;
  });

  // 监听 self_id 变化
  const { listen } = await import('@tauri-apps/api/event');
  await listen<number>('runbot-self-id', (event) => {
    state.selfId = event.payload;
  });

  state.initialized = true;
}

/**
 * 获取只读的连接状态
 * 返回响应式的状态对象，可以直接在模板中使用
 */
export function useConnectionState() {
  return {
    status: readonly(state.status),
    selfId: computed(() => state.selfId),
    initialized: computed(() => state.initialized),
  };
}

/**
 * 获取连接状态（响应式）
 * 注意：直接修改返回的状态不会生效，应该通过 runbotService 的方法来改变状态
 */
export function getConnectionState() {
  return state;
}


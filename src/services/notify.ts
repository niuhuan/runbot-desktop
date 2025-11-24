/**
 * macOS 消息通知服务
 * 使用 Tauri 通知插件
 */

import { getCurrentWindow } from '@tauri-apps/api/window';
import { isPermissionGranted, requestPermission, sendNotification } from '@tauri-apps/plugin-notification';

let throttleTimer: number | null = null;
let permissionGranted = false;

/**
 * 初始化通知权限
 */
export async function initNotificationPermission(): Promise<void> {
  try {
    // 检查权限
    permissionGranted = await isPermissionGranted();
    
    if (!permissionGranted) {
      // 请求权限
      console.log('[notify] 请求通知权限...');
      const permission = await requestPermission();
      permissionGranted = permission === 'granted';
      console.log('[notify] 通知权限结果:', permissionGranted ? '已授予' : '被拒绝');
    } else {
      console.log('[notify] 通知权限已授予');
    }
  } catch (e) {
    console.warn('[notify] 初始化权限失败:', e);
  }
}

interface NotifyOptions {
  title: string;
  body: string;
  force?: boolean;
}

/**
 * 发送新消息通知
 * 策略：
 * 1. 窗口已聚焦 -> 不提醒
 * 2. 未获取权限 -> 跳过
 * 3. 5秒节流（可通过 force 跳过）
 */
export async function notifyNewMessage({ title, body, force = false }: NotifyOptions): Promise<void> {
  try {
    // 检查窗口是否聚焦
    const win = getCurrentWindow();
    const focused = await win.isFocused();
    if (focused) {
      console.log('[notify] 窗口已聚焦，跳过通知');
      return;
    }

    // 检查权限
    if (!permissionGranted) {
      // 尝试再次请求权限
      await initNotificationPermission();
      if (!permissionGranted) {
        console.log('[notify] 没有通知权限，跳过通知');
        return;
      }
    }

    // 节流：5秒内只发一次
    if (!force && throttleTimer) {
      console.log('[notify] 节流中，跳过通知');
      return;
    }
    if (!force) {
      throttleTimer = window.setTimeout(() => {
        throttleTimer = null;
      }, 5000);
    }

    // 截断过长的消息
    const trimmedBody = body.length > 140 ? body.slice(0, 137) + '…' : body;

    console.log('[notify] 发送通知:', title, trimmedBody);
    
    // 使用 Tauri 通知插件发送通知
    sendNotification({
      title,
      body: trimmedBody,
    });
  } catch (e) {
    console.warn('[notify] 发送通知失败:', e);
  }
}

/**
 * 发送聊天消息通知的便捷方法
 */
export async function notifyChatMessage(chatName: string, messagePreview: string): Promise<void> {
  await notifyNewMessage({
    title: chatName || '新消息',
    body: messagePreview,
  });
}

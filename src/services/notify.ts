/**
 * macOS 消息通知服务
 * 使用 Web Notification API，保持最多一个通知
 */

import { getCurrentWindow } from '@tauri-apps/api/window';

let lastNotification: Notification | null = null;
let throttleTimer: number | null = null;
let permissionGranted = false;

/**
 * 初始化通知权限
 */
export async function initNotificationPermission(): Promise<void> {
  try {
    if ('Notification' in window) {
      if (Notification.permission === 'granted') {
        permissionGranted = true;
      } else if (Notification.permission === 'default') {
        const permission = await Notification.requestPermission();
        permissionGranted = permission === 'granted';
      }
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
 * 3. 同时只保持一个通知（关闭旧的）
 * 4. 5秒节流（可通过 force 跳过）
 */
export async function notifyNewMessage({ title, body, force = false }: NotifyOptions): Promise<void> {
  try {
    // 检查窗口是否聚焦
    const win = getCurrentWindow();
    const focused = await win.isFocused();
    if (focused) return;

    // 检查权限
    if (!permissionGranted) {
      // 尝试再次请求权限
      await initNotificationPermission();
      if (!permissionGranted) return;
    }

    // 节流：5秒内只发一次
    if (!force && throttleTimer) return;
    if (!force) {
      throttleTimer = window.setTimeout(() => {
        throttleTimer = null;
      }, 5000);
    }

    // 截断过长的消息
    const trimmedBody = body.length > 140 ? body.slice(0, 137) + '…' : body;

    // 关闭旧通知
    if (lastNotification) {
      try {
        lastNotification.close();
      } catch {}
    }

    // 创建新通知
    lastNotification = new Notification(title, {
      body: trimmedBody,
      silent: false,
    });

    // 点击通知时聚焦窗口
    lastNotification.onclick = () => {
      window.focus();
      try {
        lastNotification?.close();
      } catch {}
    };
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

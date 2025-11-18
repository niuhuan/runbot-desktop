/**
 * 头像服务
 * 提供头像获取和缓存功能
 */

import { invoke } from '@tauri-apps/api/core';

/**
 * 获取用户头像 URL（带缓存）
 * @param userId QQ 号
 * @param selfId 当前登录用户的 QQ 号（用于多用户数据隔离）
 * @returns 头像的本地路径或 URL
 */
export async function getUserAvatar(userId: number, selfId?: number): Promise<string | null> {
  try {
    const avatarPath = await invoke<string | null>('get_user_avatar', {
      userId,
      selfId: selfId || null,
    });
    return avatarPath;
  } catch (error) {
    console.error('获取用户头像失败:', error);
    return null;
  }
}

/**
 * 获取群组头像 URL（带缓存）
 * @param groupId 群组 ID
 * @param selfId 当前登录用户的 QQ 号（用于多用户数据隔离）
 * @returns 头像的本地路径或 URL
 */
export async function getGroupAvatar(groupId: number, selfId?: number): Promise<string | null> {
  try {
    const avatarPath = await invoke<string | null>('get_group_avatar', {
      groupId,
      selfId: selfId || null,
    });
    return avatarPath;
  } catch (error) {
    console.error('获取群组头像失败:', error);
    return null;
  }
}

/**
 * 清除头像缓存
 * @param selfId 当前登录用户的 QQ 号（用于多用户数据隔离）
 */
export async function clearAvatarCache(selfId?: number): Promise<void> {
  try {
    await invoke('clear_avatar_cache', {
      selfId: selfId || null,
    });
  } catch (error) {
    console.error('清除头像缓存失败:', error);
  }
}


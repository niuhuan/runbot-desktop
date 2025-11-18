/**
 * 图片服务
 * 下载和缓存消息中的图片
 */

import { invoke } from '@tauri-apps/api/core';

/**
 * 下载并缓存图片
 * @param url 图片 URL
 * @param selfId 当前登录用户的 QQ 号（用于多用户数据隔离）
 * @param file 图片文件标识符（可选，用于 URL 过期时重新获取）
 * @returns 缓存的图片路径（相对路径）
 */
export async function downloadImage(url: string, selfId?: number, file?: string): Promise<string | null> {
  try {
    const imagePath = await invoke<string | null>('download_image', {
      url,
      selfId: selfId || null,
      file: file || null,
    });
    return imagePath;
  } catch (error) {
    console.error('下载图片失败:', error);
    return null;
  }
}

/**
 * 检查图片缓存
 * @param url 图片 URL
 * @param selfId 当前登录用户的 QQ 号（用于多用户数据隔离）
 * @returns 缓存的图片路径（相对路径），如果不存在则返回 null
 */
export async function checkImageCache(url: string, selfId?: number): Promise<string | null> {
  try {
    const imagePath = await invoke<string | null>('check_image_cache', {
      url,
      selfId: selfId || null,
    });
    return imagePath;
  } catch (error) {
    console.error('检查图片缓存失败:', error);
    return null;
  }
}


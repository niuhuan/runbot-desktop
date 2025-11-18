/**
 * 配置管理服务
 * 使用 Rust 存储（文件系统）存储应用配置
 */

import { invoke } from '@tauri-apps/api/core';

const CONFIG_KEY = 'runbot-desktop-config';

export interface AppConfig {
  wsUrl: string;
  accessToken?: string;
  lastConnected?: boolean;
}

/**
 * 保存配置（用户特定）
 */
export async function saveConfig(config: AppConfig, selfId?: number): Promise<void> {
  try {
    await invoke('save_config', {
      key: CONFIG_KEY,
      value: JSON.stringify(config),
      selfId: selfId || null,
    });
  } catch (error) {
    console.error('保存配置失败:', error);
    throw error;
  }
}

/**
 * 读取配置（用户特定）
 */
export async function loadConfig(selfId?: number): Promise<AppConfig | null> {
  try {
    const configStr = await invoke<string | null>('load_config', {
      key: CONFIG_KEY,
      selfId: selfId || null,
    });
    
    if (!configStr) {
      return null;
    }
    
    return JSON.parse(configStr) as AppConfig;
  } catch (error) {
    console.error('读取配置失败:', error);
    return null;
  }
}

/**
 * 清除配置（用户特定）
 */
export async function clearConfig(selfId?: number): Promise<void> {
  try {
    await invoke('remove_config', {
      key: CONFIG_KEY,
      selfId: selfId || null,
    });
  } catch (error) {
    console.error('清除配置失败:', error);
    throw error;
  }
}

/**
 * 更新配置的部分字段（用户特定）
 */
export async function updateConfig(updates: Partial<AppConfig>, selfId?: number): Promise<void> {
  const currentConfig = await loadConfig(selfId);
  const defaultConfig: AppConfig = { wsUrl: 'ws://127.0.0.1:8080' };
  const newConfig = { ...defaultConfig, ...currentConfig, ...updates };
  await saveConfig(newConfig, selfId);
}

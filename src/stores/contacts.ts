/**
 * 全局联系人列表和群组列表管理
 * 统一维护联系人列表和群组列表，供所有组件使用
 */

import { reactive, readonly } from 'vue';

export interface Contact {
  userId: number;
  nickname: string;
  remark?: string;
}

export interface Group {
  groupId: number;
  groupName: string;
  memberCount?: number;
}

// 全局状态
const state = reactive<{
  contacts: Contact[];
  groups: Group[];
  initialized: boolean;
}>({
  contacts: [],
  groups: [],
  initialized: false,
});

/**
 * 更新联系人列表
 */
export function updateContacts(contacts: Contact[]): void {
  state.contacts = contacts;
}

/**
 * 更新群组列表
 */
export function updateGroups(groups: Group[]): void {
  state.groups = groups;
}

/**
 * 根据用户ID获取联系人
 */
export function getContact(userId: number): Contact | undefined {
  return state.contacts.find(c => c.userId === userId);
}

/**
 * 根据群组ID获取群组
 */
export function getGroup(groupId: number): Group | undefined {
  return state.groups.find(g => g.groupId === groupId);
}

/**
 * 根据用户ID获取联系人名称（优先备注，其次昵称，最后默认格式）
 */
export function getContactName(userId: number): string {
  const contact = getContact(userId);
  if (contact) {
    return contact.remark || contact.nickname || `用户 ${userId}`;
  }
  return `用户 ${userId}`;
}

/**
 * 根据群组ID获取群组名称
 */
export function getGroupName(groupId: number): string {
  const group = getGroup(groupId);
  if (group && group.groupName) {
    return group.groupName;
  }
  return `群组 ${groupId}`;
}

/**
 * 获取只读的联系人列表和群组列表
 * 返回响应式的状态对象，可以直接在模板中使用
 */
export function useContactsState() {
  return {
    contacts: readonly(state.contacts),
    groups: readonly(state.groups),
    get initialized() {
      return state.initialized;
    },
  };
}

/**
 * 获取联系人列表和群组列表（响应式）
 */
export function getContactsState() {
  return state;
}

/**
 * 初始化联系人列表和群组列表管理
 */
export function initContactsStore(): void {
  state.initialized = true;
}


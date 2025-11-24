import { reactive } from 'vue';

/**
 * 群成员信息接口
 */
export interface GroupMemberInfo {
  groupId: number;
  userId: number;
  nickname: string;
  card?: string; // 群名片
  role?: string; // owner, admin, member
  joinTime?: number;
  lastSentTime?: number;
  level?: string;
  title?: string;
}

/**
 * 群成员缓存状态
 */
interface GroupMembersState {
  // 群成员映射: groupId -> userId -> GroupMemberInfo
  members: Map<number, Map<number, GroupMemberInfo>>;
  // 群成员列表加载时间: groupId -> timestamp
  loadTime: Map<number, number>;
}

// 缓存有效期：30分钟
const CACHE_EXPIRE_TIME = 30 * 60 * 1000;

const state = reactive<GroupMembersState>({
  members: new Map(),
  loadTime: new Map(),
});

/**
 * 获取群成员信息
 */
export function getGroupMember(groupId: number, userId: number): GroupMemberInfo | undefined {
  return state.members.get(groupId)?.get(userId);
}

/**
 * 获取群成员显示名称（优先群名片，其次昵称）
 */
export function getGroupMemberDisplayName(groupId: number, userId: number): string {
  const member = getGroupMember(groupId, userId);
  if (!member) {
    return `用户 ${userId}`;
  }
  return member.card || member.nickname || `用户 ${userId}`;
}

/**
 * 获取群所有成员
 */
export function getGroupMembers(groupId: number): GroupMemberInfo[] {
  const members = state.members.get(groupId);
  if (!members) {
    return [];
  }
  return Array.from(members.values());
}

/**
 * 更新群成员信息（单个）
 */
export function updateGroupMember(groupId: number, member: GroupMemberInfo): void {
  if (!state.members.has(groupId)) {
    state.members.set(groupId, new Map());
  }
  state.members.get(groupId)!.set(member.userId, member);
}

/**
 * 更新群成员列表（批量）
 */
export function updateGroupMembers(groupId: number, members: GroupMemberInfo[]): void {
  const memberMap = new Map<number, GroupMemberInfo>();
  for (const member of members) {
    memberMap.set(member.userId, member);
  }
  state.members.set(groupId, memberMap);
  state.loadTime.set(groupId, Date.now());
}

/**
 * 检查群成员缓存是否过期
 */
export function isGroupMembersCacheExpired(groupId: number): boolean {
  const loadTime = state.loadTime.get(groupId);
  if (!loadTime) {
    return true;
  }
  return Date.now() - loadTime > CACHE_EXPIRE_TIME;
}

/**
 * 清除群成员缓存
 */
export function clearGroupMembersCache(groupId: number): void {
  state.members.delete(groupId);
  state.loadTime.delete(groupId);
}

/**
 * 清除所有群成员缓存
 */
export function clearAllGroupMembersCache(): void {
  state.members.clear();
  state.loadTime.clear();
}

/**
 * 获取状态（用于调试）
 */
export function getGroupMembersState() {
  return state;
}

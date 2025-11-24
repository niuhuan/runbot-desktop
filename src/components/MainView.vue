<script setup lang="ts">
import { ref, onMounted, onBeforeUnmount, nextTick, watch } from 'vue';
import { listen } from '@tauri-apps/api/event';
import { runbotService } from '../services/runbot';
import { useConnectionState, initConnectionStore, getConnectionState } from '../stores/connection';
import { initContactsStore, getContactName, getGroupName } from '../stores/contacts';
import { initChatsStore, updateChatFromMessage, updateChatList, clearUnreadCount } from '../stores/chats';
import { useRequestsStore } from '../stores/requests';
import { updateConfig } from '../services/config';
import { saveMessage } from '../services/storage';
import { initNotificationPermission, notifyChatMessage } from '../services/notify';
import ChatList from './ChatList.vue';
import ContactList from './ContactList.vue';
import GroupList from './GroupList.vue';
import ChatArea from './ChatArea.vue';
import RequestList from './RequestList.vue';

const emit = defineEmits<{
  disconnect: []
}>();

// 使用全局连接状态
const { status: connectionStatus } = useConnectionState();
const requestsStore = useRequestsStore();
const selfId = ref<number | null>(null);
const selfAvatar = ref<string | null>(null);
const selfAvatarFailed = ref(false);
const selfOnlineStatus = ref<number | null>(null); // 在线状态：0/1=在线, 2=隐身, 3=离线, 4=忙碌, 5=离开
const showAvatarMenu = ref(false);
const avatarButtonRef = ref<HTMLElement | null>(null);
const showMainMenu = ref(false);
const menuButtonRef = ref<HTMLElement | null>(null);
const showStatusSubMenu = ref(false);
const statusSubMenuRef = ref<HTMLElement | null>(null);
const statusSubMenuTimeout = ref<number | null>(null);

// 在线状态选项
const onlineStatusOptions = [
  { value: 1, label: '在线', color: '#4caf50' },
  { value: 2, label: '隐身', color: '#9e9e9e' },
  { value: 4, label: '忙碌', color: '#ff9800' },
  { value: 5, label: '离开', color: '#ffc107' },
];

// 联系人列表和群组列表（用于向后兼容，实际数据在全局 store 中）
const contactsList = ref<Array<{ userId: number; nickname: string; remark?: string }>>([]);
const groupsList = ref<Array<{ groupId: number; groupName: string }>>([]);

// 左侧列表类型：'chat' | 'contact' | 'group' | 'request'
const leftPanelType = ref<'chat' | 'contact' | 'group' | 'request'>('chat');

// 监听左侧面板类型变化，当打开联系人或群组列表时，主动更新数据
watch(leftPanelType, async (newType) => {
  try {
    // 使用 nextTick 确保组件已经挂载
    await nextTick();
    
    if (newType === 'contact' && contactListRef.value) {
      // 打开联系人列表时，主动加载联系人数据
      await contactListRef.value.loadContacts();
    } else if (newType === 'group' && groupListRef.value) {
      // 打开群组列表时，主动加载群组数据
      await groupListRef.value.loadGroups();
    }
  } catch (error) {
    console.error('加载列表数据失败:', error);
  }
});

// 当前选中的聊天
const currentChat = ref<{
  type: 'private' | 'group' | null;
  id: number | null;
  name: string;
}>({
  type: null,
  id: null,
  name: '',
});

// 组件引用
const chatListRef = ref<InstanceType<typeof ChatList> | null>(null);
const contactListRef = ref<InstanceType<typeof ContactList> | null>(null);
const groupListRef = ref<InstanceType<typeof GroupList> | null>(null);
const chatAreaRef = ref<InstanceType<typeof ChatArea> | null>(null);
const requestListRef = ref<InstanceType<typeof RequestList> | null>(null);

// 断开连接
const handleDisconnect = async () => {
  try {
    // 先保存当前的 selfId（在清空之前）
    const currentSelfId = selfId.value;
    
    // 在断开连接之前，先更新配置，清除自动登录标志
    // 这样确保配置已经写入文件系统，即使断开连接失败也能正确保存状态
    console.log('[MainView] 退出登录：正在清除自动登录标志...');
    try {
      // 同时清除用户特定配置和全局配置（因为登录页面可能读取全局配置）
      if (currentSelfId) {
        await updateConfig({ lastConnected: false }, currentSelfId);
        console.log('[MainView] 已清除用户特定配置的自动登录标志 (selfId:', currentSelfId, ')');
      }
      // 也清除全局配置（不使用 selfId）
      await updateConfig({ lastConnected: false });
      console.log('[MainView] 已清除全局配置的自动登录标志');
    } catch (configError) {
      console.error('[MainView] 清除自动登录标志失败:', configError);
      // 即使配置更新失败，也继续执行断开连接
    }
    
    // 确保配置已经写入完成（等待文件系统同步）
    await new Promise(resolve => setTimeout(resolve, 200));
    
    // 然后断开连接
    await runbotService.disconnect();
    console.log('[MainView] 已断开连接');
    
    // 清空状态
    selfId.value = null;
    currentChat.value = { type: null, id: null, name: '' };
    
    // 最后触发 disconnect 事件，切换到登录页面
    emit('disconnect');
  } catch (error) {
    console.error('断开连接失败:', error);
    // 即使断开连接失败，也要清除自动登录标志
    const currentSelfId = selfId.value;
    try {
      if (currentSelfId) {
        await updateConfig({ lastConnected: false }, currentSelfId);
      }
      await updateConfig({ lastConnected: false });
      console.log('[MainView] 已清除自动登录标志（错误处理）');
      // 等待配置写入完成
      await new Promise(resolve => setTimeout(resolve, 200));
    } catch (configError) {
      console.error('[MainView] 清除自动登录标志失败:', configError);
    }
    // 清空状态
    selfId.value = null;
    currentChat.value = { type: null, id: null, name: '' };
    // 仍然触发 disconnect 事件
    emit('disconnect');
  }
};

// 显示状态子菜单
const handleStatusSubMenuEnter = () => {
  // 清除可能存在的延迟关闭定时器
  if (statusSubMenuTimeout.value !== null) {
    clearTimeout(statusSubMenuTimeout.value);
    statusSubMenuTimeout.value = null;
  }
  showStatusSubMenu.value = true;
};

// 延迟关闭状态子菜单
const handleStatusSubMenuLeave = () => {
  // 延迟关闭，给用户时间移动到子菜单
  statusSubMenuTimeout.value = window.setTimeout(() => {
    showStatusSubMenu.value = false;
    statusSubMenuTimeout.value = null;
  }, 200);
};

// 设置在线状态
const handleSetOnlineStatus = async (status: number) => {
  // 清除定时器
  if (statusSubMenuTimeout.value !== null) {
    clearTimeout(statusSubMenuTimeout.value);
    statusSubMenuTimeout.value = null;
  }
  showStatusSubMenu.value = false;
  showAvatarMenu.value = false;
  
  try {
    await runbotService.setOnlineStatus(status);
    // 更新本地状态
    selfOnlineStatus.value = status;
    console.log('在线状态已设置为:', status);
  } catch (error) {
    console.error('设置在线状态失败:', error);
  }
};

// 获取状态子菜单位置
const getStatusSubMenuStyle = () => {
  if (!statusSubMenuRef.value) {
    return {};
  }
  const rect = statusSubMenuRef.value.getBoundingClientRect();
  // 子菜单从主菜单右侧弹出
  return {
    top: `${rect.top}px`,
    left: `${rect.right + 8}px`,
  };
};

// 头像菜单中断开连接
const handleAvatarMenuDisconnect = () => {
  showAvatarMenu.value = false;
  handleDisconnect();
};

// 关闭所有菜单
const closeAllMenus = () => {
  showAvatarMenu.value = false;
  showMainMenu.value = false;
  showStatusSubMenu.value = false;
};

// 获取头像菜单位置
const getAvatarMenuStyle = () => {
  if (!avatarButtonRef.value) {
    return {};
  }
  const rect = avatarButtonRef.value.getBoundingClientRect();
  // 菜单从头像下方弹出
  return {
    top: `${rect.bottom + 8}px`,
    left: `${rect.left}px`,
  };
};

// 获取主菜单位置
const getMenuStyle = () => {
  if (!menuButtonRef.value) {
    return {};
  }
  const rect = menuButtonRef.value.getBoundingClientRect();
  // 菜单从按钮上方弹出
  return {
    bottom: `${window.innerHeight - rect.top + 8}px`,
    left: `${rect.left + rect.width + 8}px`,
  };
};

// 选择聊天
const handleSelectChat = (chat: { type: 'private' | 'group'; userId?: number; groupId?: number; name: string }) => {
  currentChat.value = {
    type: chat.type,
    id: chat.type === 'private' ? chat.userId! : chat.groupId!,
    name: chat.name,
  };
  
  // 清除该对话的未读消息数
  const chatId = chat.type === 'private' ? `private_${chat.userId}` : `group_${chat.groupId}`;
  clearUnreadCount(chatId);
};

// 选择联系人
const handleSelectContact = (contact: { userId: number; nickname: string }) => {
  currentChat.value = {
    type: 'private',
    id: contact.userId,
    name: contact.nickname,
  };
  
  // 清除该对话的未读消息数
  clearUnreadCount(`private_${contact.userId}`);
};

// 选择群组
const handleSelectGroup = (group: { groupId: number; groupName: string }) => {
  currentChat.value = {
    type: 'group',
    id: group.groupId,
    name: group.groupName,
  };
  
  // 清除该对话的未读消息数
  clearUnreadCount(`group_${group.groupId}`);
  currentChat.value = {
    type: 'group',
    id: group.groupId,
    name: group.groupName,
  };
};

// 监听状态变化
let statusUnlisten: (() => void) | null = null;
let messageUnlisten: (() => void) | null = null;

onMounted(async () => {
  // 初始化全局连接状态管理
  await initConnectionStore();
  
  // 初始化全局联系人列表和群组列表管理
  initContactsStore();
  
  // 初始化全局对话列表管理
  initChatsStore();
  
  // 初始化通知权限
  initNotificationPermission();

  // 同步全局 self_id 到本地 ref（用于向后兼容）
  const connectionState = getConnectionState();
  selfId.value = connectionState.selfId;

  // 监听连接状态变化（通过全局状态）
  statusUnlisten = await runbotService.onStatusChange(async (status) => {
    if (status.status === 'disconnected' || status.status === 'error') {
      emit('disconnect');
    } else if (status.status === 'connected' && selfId.value) {
      // 连接成功后加载历史请求
      await requestsStore.loadRequests();
      // 补充请求的用户名和群组名
      requestsStore.updateRequestNames(getContactName, getGroupName);
      
      // 连接成功且有 self_id 时，先立即加载对话列表，然后再获取联系人列表和群组列表
      try {
        // 1. 先立即加载对话列表（从数据库）
        console.log('[MainView] 连接成功且有 self_id，先加载对话列表（从数据库）');
        await updateChatList(selfId.value);
        
        // 2. 然后获取联系人列表和群组列表
        console.log('[MainView] 连接成功且有 self_id，主动加载联系人列表和群组列表');
        await runbotService.getFriendList();
        await runbotService.getGroupList();
        // 等待一下，确保数据已经更新到全局 store
        await nextTick();
        
        // 3. 最后再次更新全局对话列表（因为联系人/群组名称可能已更新）
        console.log('[MainView] 联系人/群组列表已更新，重新加载对话列表');
        await updateChatList(selfId.value);
      } catch (error) {
        console.error('[MainView] 加载联系人/群组列表失败:', error);
      }
    }
  });

  // 监听 self_id 更新事件
  const selfIdUnlisten = await listen<number>('runbot-self-id', async (event) => {
    selfId.value = event.payload;
    // 获取到 self_id 后，先立即加载对话列表，然后再获取联系人列表和群组列表
    if (selfId.value && connectionStatus.status === 'connected') {
      try {
        // 1. 先立即加载对话列表（从数据库）
        console.log('[MainView] 获取到 self_id，先加载对话列表（从数据库）');
        await updateChatList(selfId.value);
        
        // 2. 然后获取联系人列表和群组列表
        console.log('[MainView] 获取到 self_id，主动加载联系人列表和群组列表');
        await runbotService.getFriendList();
        await runbotService.getGroupList();
        // 等待一下，确保数据已经更新到全局 store
        await nextTick();
        
        // 3. 最后再次更新全局对话列表（因为联系人/群组名称可能已更新）
        console.log('[MainView] 联系人/群组列表已更新，重新加载对话列表');
        await updateChatList(selfId.value);
      } catch (error) {
        console.error('[MainView] 加载联系人/群组列表失败:', error);
      }
    }
  });
  
  // 在组件挂载时，如果已经连接且有 self_id，先立即加载对话列表（从数据库）
  // 然后再获取联系人列表和群组列表，最后再次更新对话列表（因为名称可能已更新）
  if (selfId.value && connectionStatus.status === 'connected') {
    try {
      // 1. 先立即加载对话列表和请求列表（从数据库），让用户能立即看到内容
      console.log('[MainView] 组件挂载时，先加载对话列表（从数据库）');
      await updateChatList(selfId.value);
      
      console.log('[MainView] 组件挂载时，加载请求列表（从数据库）');
      await requestsStore.loadRequests();
      
      // 2. 然后获取联系人列表和群组列表
      console.log('[MainView] 组件挂载时，主动加载联系人列表和群组列表');
      await runbotService.getFriendList();
      await runbotService.getGroupList();
      // 等待一下，确保数据已经更新到全局 store
      await nextTick();
      
      // 3. 最后再次更新全局对话列表和请求列表的名称（因为联系人/群组名称可能已更新）
      console.log('[MainView] 组件挂载时，联系人/群组列表已更新，重新加载对话列表');
      await updateChatList(selfId.value);
      
      console.log('[MainView] 组件挂载时，更新请求列表的名称');
      requestsStore.updateRequestNames(getContactName, getGroupName);
    } catch (error) {
      console.error('[MainView] 组件挂载时加载失败:', error);
    }
  }

  // 监听消息
  messageUnlisten = await runbotService.onMessage(async (message) => {
    // [调试] 打印所有收到的消息
    console.log('[MainView] 收到原始消息:', {
      post_type: message.post_type,
      message_type: message.message_type,
      sub_type: message.sub_type,
      notice_type: message.notice_type,
      message_id: message.message_id,
      raw: message.raw
    });
    
    // 从消息中提取 self_id（如果还没有）
    if (!selfId.value && message.self_id) {
      selfId.value = message.self_id;
    }
    
    // 处理 API 响应（OneBot v11 API 响应格式）
    // API 响应可能包含 status, retcode, data, echo, action 字段
    // 或者 post_type === 'api_response'（后端标记的）
    const isApiResponse = message.post_type === 'api_response' ||
                         message.status !== undefined || 
                         message.retcode !== undefined || 
                         message.echo !== undefined ||
                         (message.raw && typeof message.raw === 'object' && 
                          (message.raw as any).status !== undefined);
    
    if (isApiResponse) {
      // 尝试从不同位置获取响应数据
      let responseData: any = null;
      let action: string | null = null;
      
      // 方式1: 从 message.raw 获取（后端已解析的 API 响应）
      if (message.raw && typeof message.raw === 'object') {
        const raw = message.raw as any;
        responseData = raw.data;
        // 优先从 raw.action 获取（后端已添加）
        action = raw.action || message.action || null;
        
        // 如果还没有 action，尝试从 echo 中提取
        if (!action && raw.echo) {
          const echoStr = String(raw.echo);
          if (echoStr.startsWith('get_friend_list_')) {
            action = 'get_friend_list';
          } else if (echoStr.startsWith('get_group_list_')) {
            action = 'get_group_list';
          }
        }
      }
      // 方式2: 直接从 message.data 获取
      else if (message.data) {
        responseData = message.data;
        action = message.action || null;
      }
      
      // 处理好友列表响应
      // 检查 action 或通过 echo 匹配（如果 echo 包含请求信息）
      const isFriendListResponse = action === 'get_friend_list' || 
                                   (message.echo && message.echo.includes('get_friend_list')) ||
                                   (message.raw && (message.raw as any).echo && 
                                    String((message.raw as any).echo).includes('get_friend_list'));
      
      if (isFriendListResponse && responseData && Array.isArray(responseData)) {
        const contacts = responseData.map((item: any) => ({
          userId: item.user_id,
          nickname: item.nickname || `用户 ${item.user_id}`,
          remark: item.remark,
        }));
        // 更新全局联系人列表
        const { updateContacts } = await import('../stores/contacts');
        updateContacts(contacts);
        // 同时更新本地引用（用于向后兼容）
        contactsList.value = contacts;
        setTimeout(() => {
          if (contactListRef.value) {
            contactListRef.value.updateContacts(contacts);
          }
        }, 100);
      }
      // 处理群组列表响应
      const isGroupListResponse = action === 'get_group_list' || 
                                 (message.echo && message.echo.includes('get_group_list')) ||
                                 (message.raw && (message.raw as any).echo && 
                                  String((message.raw as any).echo).includes('get_group_list'));
      
      if (isGroupListResponse && responseData && Array.isArray(responseData)) {
        const groups = responseData.map((item: any) => ({
          groupId: item.group_id,
          groupName: item.group_name || `群组 ${item.group_id}`,
          memberCount: item.member_count,
        }));
        // 更新全局群组列表
        const { updateGroups } = await import('../stores/contacts');
        updateGroups(groups);
        // 同时更新本地引用（用于向后兼容）
        groupsList.value = groups;
        setTimeout(async () => {
          if (groupListRef.value) {
            groupListRef.value.updateGroups(groups);
          }
          // 更新全局对话列表，确保群组名称正确显示
          await updateChatList(selfId.value || undefined);
        }, 100);
      }
      
      // 处理用户状态响应
      const isUserStatusResponse = action === 'get_user_status' ||
                                   (message.echo && message.echo.includes('get_user_status')) ||
                                   (message.raw && (message.raw as any).echo && 
                                    String((message.raw as any).echo).includes('get_user_status'));
      
      if (isUserStatusResponse && responseData) {
        console.log('[MainView] 收到用户状态响应:', responseData);
        // 检查是否是自己的状态
        if (responseData.user_id === selfId.value) {
          console.log('[MainView] 更新自己的在线状态:', responseData.status);
          selfOnlineStatus.value = responseData.status;
        }
      }
      
      // 如果不是事件消息，跳过后续处理（API 响应不需要保存到数据库或显示在聊天区域）
      if (message.post_type !== 'message' && 
          message.post_type !== 'notice' && 
          message.post_type !== 'request') {
        return;
      }
    }
    
    // 为所有消息生成 localMessageId（如果还没有）
    if (!message.localMessageId) {
      // 生成 UUID v4
      message.localMessageId = 'xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx'.replace(/[xy]/g, (c) => {
        const r = Math.random() * 16 | 0;
        const v = c === 'x' ? r : (r & 0x3 | 0x8);
        return v.toString(16);
      });
    }
    
    // 处理发送的消息（MessageSent 事件）
    if (message.post_type === 'message_sent') {
      // 发送的消息也需要保存和显示
      try {
        await saveMessage(message, selfId.value || undefined);
      } catch (error) {
        console.error('保存发送的消息失败:', error);
      }
      
      // 传递给聊天区域（ChatArea 会处理显示和去重）
      await nextTick();
      if (chatAreaRef.value) {
        chatAreaRef.value.addMessage(message);
      }
      
      // 更新全局对话列表
      updateChatFromMessage(message, selfId.value || undefined);
      
      return; // 发送的消息不需要后续处理
    }
    
    // 处理撤回消息通知（在保存之前处理，避免 saveMessage 异常导致无法处理撤回）
    console.log('[MainView] 收到消息:', message.post_type, message.sub_type, message);
    if (message.post_type === 'notice' && (message.sub_type === 'group_recall' || message.sub_type === 'friend_recall')) {
      console.log('[MainView] 检测到撤回消息通知, sub_type:', message.sub_type);
      // 从 raw 中获取 message_id
      // raw 的结构是 { GroupRecall: {...} } 或 { FriendRecall: {...} }
      const raw = message.raw as any;
      let messageId: number | undefined;
      
      if (raw?.GroupRecall?.message_id) {
        messageId = raw.GroupRecall.message_id;
      } else if (raw?.FriendRecall?.message_id) {
        messageId = raw.FriendRecall.message_id;
      }
      
      console.log('[MainView] 从 raw 中提取 message_id:', messageId, 'raw:', raw);
      
      if (messageId) {
        console.log(`[MainView] 收到撤回消息通知: message_id=${messageId}, sub_type=${message.sub_type}`);
        
        try {
          // 标记消息为已撤回
          const { markMessageRecalled } = await import('../services/storage');
          await markMessageRecalled(messageId, selfId.value || undefined);
          console.log('[MainView] 已标记消息为已撤回');
          
          // 通知 ChatArea 更新显示
          await nextTick();
          if (chatAreaRef.value) {
            console.log('[MainView] 调用 ChatArea.handleMessageRecalled');
            chatAreaRef.value.handleMessageRecalled(messageId);
          } else {
            console.warn('[MainView] chatAreaRef.value 为空');
          }
        } catch (error) {
          console.error('处理撤回消息失败:', error);
        }
      } else {
        console.warn('[MainView] message_id 为空，无法处理撤回');
      }
      
      // 撤回通知不需要后续的保存、显示和通知处理
      return;
    }
    
    // 处理好友请求和群组请求
    if (message.post_type === 'request') {
      console.log('[MainView] 收到请求事件:', message);
      
      // 后端已经保存到数据库，这里只需要重新加载请求列表
      if (selfId.value) {
        try {
          // requestsStore 会自动使用全局的 selfId
          await requestsStore.loadRequests();
          // 补充请求的用户名和群组名
          requestsStore.updateRequestNames(getContactName, getGroupName);
          console.log('[MainView] 已重新加载请求列表');
        } catch (error) {
          console.error('[MainView] 重新加载请求列表失败:', error);
        }
      }
      
      // 请求事件不需要后续的保存、显示和通知处理
      return;
    }
    
    // 保存到数据库（使用 self_id）- 只处理接收的消息
    console.log('[MainView] 准备保存消息到数据库:', message.post_type);
    if (message.post_type === 'message' || message.post_type === 'notice' || message.post_type === 'request') {
      try {
        await saveMessage(message, selfId.value || undefined);
        console.log('[MainView] 消息已保存到数据库');
      } catch (error) {
        console.error('保存消息到数据库失败:', error);
      }
    }
    
    // 传递给聊天区域
    await nextTick();
    if (chatAreaRef.value) {
      chatAreaRef.value.addMessage(message);
    }
    
    // 更新全局对话列表
    updateChatFromMessage(message, selfId.value || undefined);
    
    // 发送通知（仅针对接收的消息，不是自己发送的）
    // 注意：需要排除自己发送的消息，只对别人发来的消息发送通知
    if (message.post_type === 'message' && message.message_type && message.user_id !== selfId.value) {
      let chatName = '新消息';
      if (message.message_type === 'private' && message.user_id) {
        chatName = getContactName(message.user_id);
      } else if (message.message_type === 'group' && message.group_id) {
        chatName = getGroupName(message.group_id);
      }
      const preview = (message.raw_message || message.message || '').replace(/\[CQ:[^\]]+\]/g, '').replace(/\s+/g, ' ').trim();
      if (preview) {
        notifyChatMessage(chatName, preview);
      } else {
        notifyChatMessage(chatName, '[新消息]');
      }
    }
  });

  onBeforeUnmount(() => {
    if (selfIdUnlisten) {
      selfIdUnlisten();
    }
  });
});

onBeforeUnmount(() => {
  if (statusUnlisten) statusUnlisten();
  if (messageUnlisten) messageUnlisten();
});

// 调试功能: 暴露到全局
onMounted(() => {
  (window as any).debugRecall = async (messageId: number) => {
    const { checkMessageRecalled } = await import('../services/storage');
    const result = await checkMessageRecalled(messageId, selfId.value || undefined);
    console.log('消息撤回状态:', result);
    return result;
  };
  console.log('调试函数已加载: window.debugRecall(messageId)');
});

// 获取状态颜色
const getStatusColor = (status: string) => {
  switch (status) {
    case 'connected':
      return '#4caf50';
    case 'connecting':
      return '#ff9800';
    case 'error':
      return '#f44336';
    default:
      return '#757575';
  }
};

// 获取状态文本
const getStatusText = (status: string) => {
  switch (status) {
    case 'connected':
      return '已连接';
    case 'connecting':
      return '连接中...';
    case 'error':
      return '连接错误';
    case 'disconnected':
      return '未连接';
    default:
      return '未知状态';
  }
};

// 获取头像文本（显示 QQ 号的后4位）
const getAvatarText = () => {
  if (!selfId.value) return '?';
  const idStr = String(selfId.value);
  return idStr.slice(-4);
};

// 加载自己的头像
const loadSelfAvatar = () => {
  if (!selfId.value || selfAvatar.value || selfAvatarFailed.value) {
    return;
  }
  
  selfAvatar.value = `asset://avatar/user/${selfId.value}.png`;
  selfAvatarFailed.value = false;
};

// 处理头像加载错误
const handleSelfAvatarError = (event: Event) => {
  const img = event.target as HTMLImageElement;
  selfAvatar.value = null;
  selfAvatarFailed.value = true;
  img.style.display = 'none';
};

// 加载自己的在线状态
const loadSelfOnlineStatus = async () => {
  if (!selfId.value) {
    selfOnlineStatus.value = null;
    return;
  }
  
  try {
    console.log('[MainView] 请求获取自己的在线状态, selfId:', selfId.value);
    await runbotService.getUserStatus(selfId.value);
    // 先设置一个默认值，等待 API 响应
    // 如果 API 没有响应，至少显示一个状态
    setTimeout(() => {
      if (selfOnlineStatus.value === null) {
        console.log('[MainView] 在线状态未获取到，设置默认值');
        // 默认显示在线状态（绿色）
        selfOnlineStatus.value = 1;
      }
    }, 3000);
  } catch (error) {
    console.error('获取在线状态失败:', error);
    // 失败时设置默认值
    selfOnlineStatus.value = 1;
  }
};

// 获取状态颜色
const getOnlineStatusColor = (status: number | null): string => {
  if (status === null) return '#999'; // 未知状态 - 灰色
  switch (status) {
    case 0:
    case 1:
      return '#4caf50'; // 在线 - 绿色
    case 2:
      return '#9e9e9e'; // 隐身 - 灰色
    case 3:
      return '#757575'; // 离线 - 深灰色
    case 4:
      return '#ff9800'; // 忙碌 - 橙色
    case 5:
      return '#ffc107'; // 离开 - 黄色
    default:
      return '#999'; // 未知 - 灰色
  }
};

// 监听 selfId 变化，加载头像和在线状态
watch(() => selfId.value, (newSelfId) => {
  if (newSelfId) {
    // 重置状态，重新加载
    selfAvatar.value = null;
    selfAvatarFailed.value = false;
    loadSelfAvatar();
    loadSelfOnlineStatus();
  } else {
    selfAvatar.value = null;
    selfAvatarFailed.value = false;
    selfOnlineStatus.value = null;
  }
}, { immediate: true });
</script>

<template>
  <div class="main-container">
    <!-- 主内容区域 -->
    <div class="content-area">
      <!-- 左侧第一列：垂直导航栏 -->
      <div class="nav-sidebar">
        <!-- 用户头像 -->
        <div class="nav-avatar" v-if="selfId">
          <div 
            ref="avatarButtonRef"
            class="avatar-circle"
            @click.stop="showAvatarMenu = !showAvatarMenu"
          >
            <img 
              v-if="selfAvatar" 
              :src="selfAvatar" 
              alt="头像"
              class="avatar-image"
              @error="handleSelfAvatarError"
            />
            <span v-else class="avatar-text">{{ getAvatarText() }}</span>
            <!-- 在线状态指示器 -->
            <span 
              class="status-indicator"
              :style="{ backgroundColor: getOnlineStatusColor(selfOnlineStatus) }"
            ></span>
          </div>
        </div>
        <div class="nav-avatar" v-else>
          <div class="avatar-circle">
            <svg class="avatar-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
              <path d="M20 21v-2a4 4 0 0 0-4-4H8a4 4 0 0 0-4 4v2"></path>
              <circle cx="12" cy="7" r="4"></circle>
            </svg>
          </div>
        </div>
        
        <!-- 导航项 -->
        <div class="nav-item" :class="{ active: leftPanelType === 'chat' }" @click="leftPanelType = 'chat'">
          <svg class="nav-icon" viewBox="0 0 24 24" fill="currentColor">
            <path d="M20 2H4c-1.1 0-2 .9-2 2v18l4-4h14c1.1 0 2-.9 2-2V4c0-1.1-.9-2-2-2zm0 14H6l-2 2V4h16v12z"/>
          </svg>
          <span class="nav-label">聊天</span>
        </div>
        <div class="nav-item" :class="{ active: leftPanelType === 'contact' }" @click="leftPanelType = 'contact'">
          <svg class="nav-icon" viewBox="0 0 24 24" fill="currentColor">
            <path d="M12 12c2.21 0 4-1.79 4-4s-1.79-4-4-4-4 1.79-4 4 1.79 4 4 4zm0 2c-2.67 0-8 1.34-8 4v2h16v-2c0-2.66-5.33-4-8-4z"/>
          </svg>
          <span class="nav-label">联系人</span>
        </div>
        <div class="nav-item" :class="{ active: leftPanelType === 'group' }" @click="leftPanelType = 'group'">
          <svg class="nav-icon" viewBox="0 0 24 24" fill="currentColor">
            <path d="M16 11c1.66 0 2.99-1.34 2.99-3S17.66 5 16 5s-3 1.34-3 3 1.34 3 3 3zm-8 0c1.66 0 2.99-1.34 2.99-3S9.66 5 8 5 5 6.34 5 8s1.34 3 3 3zm0 2c-2.33 0-7 1.17-7 3.5V19h14v-2.5c0-2.33-4.67-3.5-7-3.5zm8 0c-.29 0-.62.02-.97.05 1.16.84 1.97 1.97 1.97 3.45V19h6v-2.5c0-2.33-4.67-3.5-7-3.5z"/>
          </svg>
          <span class="nav-label">群组</span>
        </div>
        <div class="nav-item" :class="{ active: leftPanelType === 'request' }" @click="leftPanelType = 'request'">
          <div class="nav-icon-wrapper">
            <svg class="nav-icon" viewBox="0 0 24 24" fill="currentColor">
              <path d="M15 12c2.21 0 4-1.79 4-4s-1.79-4-4-4-4 1.79-4 4 1.79 4 4 4zm-9-2V7H4v3H1v2h3v3h2v-3h3v-2H6zm9 4c-2.67 0-8 1.34-8 4v2h16v-2c0-2.66-5.33-4-8-4z"/>
            </svg>
            <span v-if="requestsStore.unreadCount.value > 0" class="nav-badge">{{ requestsStore.unreadCount.value }}</span>
          </div>
          <span class="nav-label">请求</span>
        </div>
        
        <!-- 底部状态和操作区域 -->
        <div class="nav-bottom">
          <!-- 连接状态指示器 -->
          <div class="connection-status">
            <div 
              class="status-block"
              :style="{ backgroundColor: getStatusColor(connectionStatus?.status || 'disconnected') }"
              :title="connectionStatus?.message || getStatusText(connectionStatus?.status || 'disconnected')"
            ></div>
          </div>
          
          <!-- 菜单按钮 -->
          <div 
            ref="menuButtonRef"
            class="nav-item menu-item" 
            :class="{ active: showMainMenu }"
            @click.stop="showMainMenu = !showMainMenu"
          >
            <svg class="nav-icon" viewBox="0 0 24 24" fill="currentColor">
              <path d="M3 18h18v-2H3v2zm0-5h18v-2H3v2zm0-7v2h18V6H3z"/>
            </svg>
          </div>
        </div>
        
        <!-- 头像菜单 -->
        <div 
          v-if="showAvatarMenu" 
          class="context-menu avatar-menu"
          :style="getAvatarMenuStyle()"
          @click.stop
        >
          <div 
            ref="statusSubMenuRef"
            class="menu-item-text menu-item-with-submenu"
            @mouseenter="handleStatusSubMenuEnter"
            @mouseleave="handleStatusSubMenuLeave"
          >
            <span>切换在线状态</span>
            <svg class="submenu-arrow" viewBox="0 0 24 24" fill="currentColor">
              <path d="M10 6L8.59 7.41 13.17 12l-4.58 4.59L10 18l6-6-6-6z"/>
            </svg>
          </div>
          <div class="menu-item-text" @click="showAvatarMenu = false">
            <span>个人资料</span>
          </div>
          <div class="menu-divider"></div>
          <div class="menu-item-text menu-item-danger" @click="handleAvatarMenuDisconnect">
            <span>断开连接</span>
          </div>
        </div>
        
        <!-- 在线状态子菜单 -->
        <div 
          v-if="showStatusSubMenu && statusSubMenuRef" 
          class="context-menu submenu status-submenu"
          :style="getStatusSubMenuStyle()"
          @mouseenter="handleStatusSubMenuEnter"
          @mouseleave="handleStatusSubMenuLeave"
          @click.stop
        >
          <div 
            v-for="option in onlineStatusOptions"
            :key="option.value"
            class="menu-item-text status-option"
            :class="{ active: selfOnlineStatus === option.value }"
            @click="handleSetOnlineStatus(option.value)"
          >
            <span class="status-dot" :style="{ backgroundColor: option.color }"></span>
            <span>{{ option.label }}</span>
          </div>
        </div>
        
        <!-- 主菜单 -->
        <div 
          v-if="showMainMenu" 
          class="context-menu"
          :style="getMenuStyle()"
          @click.stop
        >
          <div class="menu-item-text" @click="showMainMenu = false">
            <span>设置</span>
          </div>
          <div class="menu-item-text" @click="showMainMenu = false">
            <span>关于</span>
          </div>
        </div>
        
        <!-- 点击外部关闭菜单 -->
        <div 
          v-if="showAvatarMenu || showMainMenu || showStatusSubMenu" 
          class="menu-overlay" 
          @click="closeAllMenus"
        ></div>
      </div>

      <!-- 左侧第二列：列表内容 -->
      <div class="left-panel">
        <div class="panel-content">
          <ChatList
            v-if="leftPanelType === 'chat'"
            ref="chatListRef"
            :selfId="selfId || undefined"
            @selectChat="handleSelectChat"
          />
          <ContactList
            v-if="leftPanelType === 'contact'"
            ref="contactListRef"
            :selfId="selfId || undefined"
            @selectContact="handleSelectContact"
          />
          <GroupList
            v-if="leftPanelType === 'group'"
            ref="groupListRef"
            @selectGroup="handleSelectGroup"
          />
          <RequestList
            v-if="leftPanelType === 'request'"
            ref="requestListRef"
          />
        </div>
      </div>

      <!-- 右侧聊天区域 -->
      <div class="right-panel">
        <ChatArea
          ref="chatAreaRef"
          :chatType="currentChat.type"
          :chatId="currentChat.id"
          :chatName="currentChat.name"
          :selfId="selfId || undefined"
        />
      </div>
    </div>
  </div>
</template>

<style scoped>
.main-container {
  width: 100vw;
  height: 100vh;
  display: flex;
  flex-direction: column;
  background: #f5f5f5;
  font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
}

.content-area {
  flex: 1;
  display: flex;
  overflow: hidden;
}

/* 左侧第一列：垂直导航栏 */
.nav-sidebar {
  width: 60px;
  display: flex;
  flex-direction: column;
  background: #f5f5f5;
  border-right: 1px solid #e0e0e0;
  flex-shrink: 0;
  padding: 8px 0;
  align-items: center;
  height: 100%;
}

.nav-avatar {
  width: 100%;
  display: flex;
  justify-content: center;
  padding: 8px 0 12px 0;
  margin-bottom: 8px;
  border-bottom: 1px solid #e0e0e0;
}

.avatar-circle {
  width: 40px;
  height: 40px;
  border-radius: 50%;
  background: linear-gradient(135deg, #07c160 0%, #06ad56 100%);
  display: flex;
  align-items: center;
  justify-content: center;
  cursor: pointer;
  transition: all 0.2s;
  box-shadow: 0 2px 4px rgba(0, 0, 0, 0.1);
  position: relative;
}

.avatar-circle:hover {
  transform: scale(1.05);
  box-shadow: 0 4px 8px rgba(0, 0, 0, 0.15);
}

.avatar-text {
  color: white;
  font-size: 12px;
  font-weight: 600;
  user-select: none;
}

.avatar-image {
  width: 100%;
  height: 100%;
  border-radius: 50%;
  object-fit: cover;
}

.avatar-circle {
  position: relative;
}

.status-indicator {
  position: absolute;
  bottom: 0;
  right: 0;
  width: 12px;
  height: 12px;
  border-radius: 50%;
  border: 2px solid white;
  box-shadow: 0 1px 3px rgba(0, 0, 0, 0.2);
}

.avatar-icon {
  width: 20px;
  height: 20px;
  color: white;
}

.nav-item {
  width: 100%;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  padding: 12px 8px;
  cursor: pointer;
  transition: all 0.2s;
  color: #666;
  border-left: 3px solid transparent;
  margin-bottom: 4px;
}

.nav-item:hover {
  background: #e8e8e8;
  color: #333;
}

.nav-item.active {
  background: #e8e8e8;
  color: #07c160;
  border-left-color: #07c160;
}

.nav-icon {
  width: 24px;
  height: 24px;
  margin-bottom: 4px;
  flex-shrink: 0;
}

.nav-icon-wrapper {
  position: relative;
  display: flex;
  align-items: center;
  justify-content: center;
}

.nav-badge {
  position: absolute;
  top: -4px;
  right: -8px;
  min-width: 16px;
  height: 16px;
  padding: 0 4px;
  background: #f44336;
  color: white;
  font-size: 10px;
  font-weight: bold;
  border-radius: 8px;
  display: flex;
  align-items: center;
  justify-content: center;
  border: 2px solid #f5f5f5;
  box-shadow: 0 2px 4px rgba(0, 0, 0, 0.2);
}

.nav-label {
  font-size: 11px;
  font-weight: 500;
}

/* 底部状态和操作区域 */
.nav-bottom {
  margin-top: auto;
  width: 100%;
  display: flex;
  flex-direction: column;
  align-items: center;
  padding-top: 8px;
  border-top: 1px solid #e0e0e0;
}

.connection-status {
  width: 100%;
  display: flex;
  justify-content: center;
  padding: 8px 0;
  margin-bottom: 4px;
}

.status-block {
  width: 24px;
  height: 8px;
  border-radius: 4px;
  transition: background-color 0.3s;
  cursor: pointer;
}

.menu-item.active {
  background: #e8e8e8;
  color: #07c160;
}

/* 弹出菜单 */
.context-menu {
  position: fixed;
  background: white;
  border-radius: 8px;
  box-shadow: 0 4px 12px rgba(0, 0, 0, 0.15);
  min-width: 160px;
  z-index: 1000;
  padding: 4px 0;
  border: 1px solid #e0e0e0;
}

.menu-item-text {
  padding: 10px 16px;
  cursor: pointer;
  font-size: 14px;
  color: #333;
  transition: background-color 0.2s;
  user-select: none;
}

.menu-item-text:hover {
  background-color: #f5f5f5;
}

.menu-item-danger {
  color: #f44336;
}

.menu-item-danger:hover {
  background-color: #ffebee;
  color: #d32f2f;
}

.menu-divider {
  height: 1px;
  background-color: #e0e0e0;
  margin: 4px 0;
}

.menu-item-with-submenu {
  display: flex;
  justify-content: space-between;
  align-items: center;
}

.submenu-arrow {
  width: 14px;
  height: 14px;
  margin-left: 8px;
  flex-shrink: 0;
}

.submenu {
  min-width: 140px;
}

.status-option {
  display: flex;
  align-items: center;
  gap: 8px;
}

.status-option.active {
  background-color: #e3f2fd;
  color: #1976d2;
}

.status-option .status-dot {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  flex-shrink: 0;
}

.menu-overlay {
  position: fixed;
  top: 0;
  left: 0;
  right: 0;
  bottom: 0;
  z-index: 999;
  background: transparent;
}

/* 左侧第二列：列表内容 */
.left-panel {
  width: 280px;
  display: flex;
  flex-direction: column;
  background: white;
  border-right: 1px solid #e0e0e0;
  flex-shrink: 0;
}

.panel-content {
  flex: 1;
  overflow: hidden;
}

.right-panel {
  flex: 1;
  display: flex;
  flex-direction: column;
  overflow: hidden;
}
</style>

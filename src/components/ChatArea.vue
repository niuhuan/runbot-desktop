<script setup lang="ts">
import { ref, computed, watch, onMounted, nextTick, onUnmounted } from 'vue';
import { runbotService, type OneBotMessage } from '../services/runbot';
import { getMessages, saveMessage } from '../services/storage';
import { parseCQCode, type CQSegment } from '../utils/cqcode';
import { getFaceDisplayText, getFaceImageUrl } from '../utils/qq-face';
import { getGroupMemberDisplayName, getGroupMembers } from '../stores/group-members';
import qface from 'qface';
import { checkImageCache, downloadImage } from '../services/image';
import { WebviewWindow } from '@tauri-apps/api/webviewWindow';
import { getCurrentWindow, currentMonitor } from '@tauri-apps/api/window';
import { listen } from '@tauri-apps/api/event';
import { 
  getChatInputState, 
  updateChatInputState, 
  clearChatInputState,
  addMentionedUser 
} from '../stores/chat-input';

// 生成 UUID v4
function generateUUID(): string {
  return 'xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx'.replace(/[xy]/g, (c) => {
    const r = Math.random() * 16 | 0;
    const v = c === 'x' ? r : (r & 0x3 | 0x8);
    return v.toString(16);
  });
}

const props = defineProps<{
  chatType: 'private' | 'group' | null;
  chatId: number | null; // userId 或 groupId
  chatName: string;
  selfId?: number;
}>();

const messages = ref<OneBotMessage[]>([]);
const sending = ref(false);
const messagesContainer = ref<HTMLElement | null>(null);
const fileInputRef = ref<HTMLInputElement | null>(null);
const selectedImages = ref<Array<{ file: File; preview: string }>>([]);
const chatAvatarFailed = ref(false);
const isComposing = ref(false); // 输入法组合状态
const compositionEndTime = ref(0); // 输入法结束时间
const showFacePicker = ref(false); // 是否显示表情选择器
const facePickerRef = ref<HTMLElement | null>(null); // 表情选择器引用
const inputEditorRef = ref<HTMLElement | null>(null); // 富文本编辑器引用
const showDebugPanel = ref(false); // 是否显示调试面板
const debugActiveTab = ref<'messages' | 'members'>('messages'); // debug 面板活动标签
const isDev = import.meta.env.DEV; // 是否为开发环境

// @ 功能相关
const showMentionPicker = ref(false); // 是否显示 @ 选择器
const mentionPickerRef = ref<HTMLElement | null>(null); // @ 选择器引用
const mentionSearchText = ref(''); // @ 搜索关键词
const mentionPickerPosition = ref<{ top?: number; bottom?: number; left: number }>({ bottom: 0, left: 0 }); // @ 选择器位置
const selectedMentionIndex = ref(0); // 当前选中的 @ 成员索引

// 右键菜单相关
const showContextMenu = ref(false);
const contextMenuX = ref(0);
const contextMenuY = ref(0);
const contextMenuMessage = ref<OneBotMessage | null>(null);

// 回复状态
const replyToMessage = ref<OneBotMessage | null>(null);

// 过滤当前聊天的消息（包括发送的消息）
const filteredMessages = computed(() => {
  if (!props.chatId || !props.chatType) return [];
  
  return messages.value.filter(msg => {
    // 处理接收的消息和发送的消息
    if (msg.post_type === 'message' || msg.post_type === 'message_sent') {
      if (props.chatType === 'private') {
        // 私聊：必须 message_type 是 'private' 且 user_id 匹配
        return msg.message_type === 'private' && msg.user_id === props.chatId;
      } else if (props.chatType === 'group') {
        // 群组：必须 message_type 是 'group' 且 group_id 匹配
        return msg.message_type === 'group' && msg.group_id === props.chatId;
      }
    }
    return false;
  }).sort((a, b) => a.time - b.time);
});

// 获取群成员列表（用于 debug 面板）
const groupMembersList = computed(() => {
  if (props.chatType !== 'group' || !props.chatId) {
    return [];
  }
  return getGroupMembers(props.chatId);
});

// 时间段阈值（5分钟，单位：秒）
const TIME_GROUP_THRESHOLD = 5 * 60;

// 获取发送人ID（用于分组）
const getSenderId = (msg: OneBotMessage): string => {
  if (msg.post_type === 'message_sent') {
    return 'self';
  }
  return `user_${msg.user_id || 'unknown'}`;
};

// 判断两个时间戳是否在同一个时间段内
const isSameTimeGroup = (time1: number, time2: number): boolean => {
  return Math.abs(time1 - time2) <= TIME_GROUP_THRESHOLD;
};

// 分组后的消息列表
interface GroupedMessage {
  time: number;
  senderId: string;
  senderName: string;
  userId: number | null; // 发送人的 user_id（用于显示头像）
  messages: OneBotMessage[];
  showSender: boolean; // 是否显示发送人名称
  showTime: boolean; // 是否显示时间
}

const groupedMessages = computed(() => {
  const filtered = filteredMessages.value;
  if (filtered.length === 0) return [];
  
  const groups: GroupedMessage[] = [];
  let currentTimeGroup: number | null = null;
  let currentSenderGroup: { senderId: string; senderName: string; userId: number | null; messages: OneBotMessage[] } | null = null;
  
  for (let i = 0; i < filtered.length; i++) {
    const msg = filtered[i];
    const senderId = getSenderId(msg);
    const senderName = getSenderName(msg);
    const userId = msg.post_type === 'message_sent' ? null : (msg.user_id || null);
    const msgTime = msg.time;
    
    // 判断是否需要开始新的时间段
    const needNewTimeGroup = currentTimeGroup === null || !isSameTimeGroup(msgTime, currentTimeGroup);
    
    // 判断是否需要开始新的发送人组
    const needNewSenderGroup = 
      needNewTimeGroup || // 新时间段
      currentSenderGroup === null || // 第一个消息
      currentSenderGroup.senderId !== senderId || // 不同发送人
      currentSenderGroup.messages.length >= 10; // 已达到10条消息上限
    
    if (needNewTimeGroup) {
      // 保存之前的发送人组
      if (currentSenderGroup) {
        groups.push({
          time: currentTimeGroup!,
          senderId: currentSenderGroup.senderId,
          senderName: currentSenderGroup.senderName,
          userId: currentSenderGroup.userId,
          messages: currentSenderGroup.messages,
          showSender: true,
          showTime: true,
        });
      }
      
      // 开始新的时间段
      currentTimeGroup = msgTime;
      currentSenderGroup = {
        senderId,
        senderName,
        userId,
        messages: [msg],
      };
    } else if (needNewSenderGroup) {
      // 保存之前的发送人组
      if (currentSenderGroup) {
        groups.push({
          time: currentTimeGroup!,
          senderId: currentSenderGroup.senderId,
          senderName: currentSenderGroup.senderName,
          userId: currentSenderGroup.userId,
          messages: currentSenderGroup.messages,
          showSender: true,
          showTime: false, // 同一时间段内不重复显示时间
        });
      }
      
      // 开始新的发送人组
      currentSenderGroup = {
        senderId,
        senderName,
        userId,
        messages: [msg],
      };
    } else {
      // 添加到当前发送人组
      if (currentSenderGroup) {
        currentSenderGroup.messages.push(msg);
      }
    }
  }
  
  // 保存最后一个发送人组
  if (currentSenderGroup) {
    const lastGroup = groups[groups.length - 1];
    groups.push({
      time: currentTimeGroup!,
      senderId: currentSenderGroup.senderId,
      senderName: currentSenderGroup.senderName,
      userId: currentSenderGroup.userId,
      messages: currentSenderGroup.messages,
      showSender: true,
      showTime: lastGroup ? !isSameTimeGroup(currentTimeGroup!, lastGroup.time) : true,
    });
  }
  
  return groups;
});

// 加载聊天消息（包括发送的消息）
const loadChatMessages = async () => {
  if (!props.chatId || !props.chatType || !props.selfId) return;

  try {
    // 加载接收的消息
    const options: any = {
      limit: 200, // 增加限制以包含发送的消息
      selfId: props.selfId,
    };

    if (props.chatType === 'private') {
      options.userId = props.chatId;
      // 不限制 postType，这样也能加载 message_sent
    } else if (props.chatType === 'group') {
      options.groupId = props.chatId;
      // 不限制 postType，这样也能加载 message_sent
    }

    const chatMessages = await getMessages(options);
    // 过滤出当前聊天的消息（包括发送的消息）
    messages.value = chatMessages.filter(msg => {
      if (msg.post_type === 'message' || msg.post_type === 'message_sent') {
        if (props.chatType === 'private') {
          // 私聊：必须 message_type 是 'private' 且 user_id 匹配
          return msg.message_type === 'private' && msg.user_id === props.chatId;
        } else if (props.chatType === 'group') {
          // 群组：必须 message_type 是 'group' 且 group_id 匹配
          return msg.message_type === 'group' && msg.group_id === props.chatId;
        }
      }
      return false;
    });
    
    // 加载消息后滚动到底部
    scrollToBottom();
    
    // 观察新消息中的图片
    nextTick(() => {
      observeImagePlaceholders();
    });
  } catch (error) {
    console.error('加载聊天消息失败:', error);
  }
};

// 将文件转换为 base64（返回完整的 data URI，用于预览和发送）
const fileToBase64 = (file: File): Promise<{ base64: string; mimeType: string }> => {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => {
      const result = reader.result as string;
      // 提取 MIME 类型和 base64 数据
      const match = result.match(/^data:([^;]+);base64,(.+)$/);
      if (match) {
        resolve({
          mimeType: match[1],
          base64: match[2],
        });
      } else {
        // 如果没有匹配到，尝试直接提取 base64
        const base64 = result.split(',')[1] || result;
        resolve({
          mimeType: file.type || 'image/png',
          base64,
        });
      }
    };
    reader.onerror = reject;
    reader.readAsDataURL(file);
  });
};

// 处理图片选择
const handleImageSelect = async (event: Event) => {
  const target = event.target as HTMLInputElement;
  const files = target.files;
  if (!files || files.length === 0) return;

  for (let i = 0; i < files.length; i++) {
    const file = files[i];
    // 检查是否是图片文件
    if (!file.type.startsWith('image/')) {
      alert(`文件 ${file.name} 不是图片文件`);
      continue;
    }

    // 检查文件大小（限制为 10MB）
    if (file.size > 10 * 1024 * 1024) {
      alert(`图片 ${file.name} 太大，请选择小于 10MB 的图片`);
      continue;
    }

    // 创建预览
    const preview = URL.createObjectURL(file);
    selectedImages.value.push({ file, preview });
  }

  // 清空 input，以便可以重复选择同一文件
  if (fileInputRef.value) {
    fileInputRef.value.value = '';
  }
};

// 移除选中的图片
const removeSelectedImage = (index: number) => {
  const image = selectedImages.value[index];
  URL.revokeObjectURL(image.preview);
  selectedImages.value.splice(index, 1);
};

// 获取所有表情列表（只获取有图片的表情，即动态表情）
const faceList = computed(() => {
  return qface.data.filter(face => {
    // 只显示动态表情（gif），不显示静态表情
    return face.isStatic !== '1';
  }).map(face => ({
    id: face.QSid,
    name: face.QDes.substring(1), // 去掉前面的斜杠
    url: getFaceImageUrl(face.QSid),
  }));
});

// 切换表情选择器显示状态
const toggleFacePicker = () => {
  showFacePicker.value = !showFacePicker.value;
};

// 选择表情（插入到富文本编辑器）
const selectFace = (faceId: string) => {
  if (!inputEditorRef.value) return;
  
  const editor = inputEditorRef.value;
  const selection = window.getSelection();
  
  // 检查当前选区是否在编辑器内
  let isSelectionInEditor = false;
  if (selection && selection.rangeCount > 0) {
    const range = selection.getRangeAt(0);
    let container = range.commonAncestorContainer;
    // 如果是文本节点，获取其父元素
    if (container.nodeType === Node.TEXT_NODE) {
      container = container.parentNode as Node;
    }
    // 检查是否是编辑器或编辑器的子节点
    isSelectionInEditor = container === editor || editor.contains(container);
  }
  
  if (selection && selection.rangeCount > 0 && isSelectionInEditor) {
    const range = selection.getRangeAt(0);
    range.deleteContents();
    
    // 创建表情图片元素
    const faceImg = document.createElement('img');
    const faceImageUrl = getFaceImageUrl(faceId);
    if (faceImageUrl) {
      faceImg.src = faceImageUrl;
      faceImg.alt = getFaceDisplayText(faceId);
      faceImg.className = 'input-face-emoji';
      faceImg.setAttribute('data-face-id', faceId);
      faceImg.setAttribute('contenteditable', 'false');
      faceImg.style.verticalAlign = 'middle';
      faceImg.style.display = 'inline-block';
      faceImg.style.width = '20px';
      faceImg.style.height = '20px';
      
      // 插入表情图片
      range.insertNode(faceImg);
      
      // 在表情后插入一个零宽空格，方便光标定位
      const space = document.createTextNode('\u200B');
      range.setStartAfter(faceImg);
      range.insertNode(space);
      range.setStartAfter(space);
      range.collapse(true);
      
      // 更新选择范围
      selection.removeAllRanges();
      selection.addRange(range);
    } else {
      // 如果没有图片，插入文本
      const textNode = document.createTextNode(getFaceDisplayText(faceId));
      range.insertNode(textNode);
      range.setStartAfter(textNode);
      range.collapse(true);
      selection.removeAllRanges();
      selection.addRange(range);
    }
    
    // 聚焦编辑器
    editor.focus();
  } else {
    // 如果没有选择，在末尾插入
    const faceImg = document.createElement('img');
    const faceImageUrl = getFaceImageUrl(faceId);
    if (faceImageUrl) {
      faceImg.src = faceImageUrl;
      faceImg.alt = getFaceDisplayText(faceId);
      faceImg.className = 'input-face-emoji';
      faceImg.setAttribute('data-face-id', faceId);
      faceImg.setAttribute('contenteditable', 'false');
      faceImg.style.verticalAlign = 'middle';
      faceImg.style.display = 'inline-block';
      faceImg.style.width = '20px';
      faceImg.style.height = '20px';
      editor.appendChild(faceImg);
      const space = document.createTextNode('\u200B');
      editor.appendChild(space);
      
      // 移动光标到末尾
      const range = document.createRange();
      range.selectNodeContents(editor);
      range.collapse(false);
      const sel = window.getSelection();
      if (sel) {
        sel.removeAllRanges();
        sel.addRange(range);
      }
    }
  }
  
  // 关闭表情选择器
  showFacePicker.value = false;
};

// 从富文本编辑器提取内容（将表情图片转换为 CQ 码）
const extractContentFromEditor = (): string => {
  if (!inputEditorRef.value) return '';
  
  const editor = inputEditorRef.value;
  let result = '';
  
  // 遍历所有子节点
  const walker = document.createTreeWalker(
    editor,
    NodeFilter.SHOW_TEXT | NodeFilter.SHOW_ELEMENT,
    {
      acceptNode: function(node) {
        // 如果节点的父元素是 input-mention，跳过（避免重复提取 @ 文本）
        if (node.parentElement && node.parentElement.classList.contains('input-mention')) {
          return NodeFilter.FILTER_REJECT;
        }
        return NodeFilter.FILTER_ACCEPT;
      }
    }
  );
  
  let node;
  while (node = walker.nextNode()) {
    if (node.nodeType === Node.TEXT_NODE) {
      // 文本节点，直接添加（跳过零宽空格）
      const text = node.textContent || '';
      result += text.replace(/\u200B/g, '');
    } else if (node.nodeType === Node.ELEMENT_NODE) {
      const element = node as HTMLElement;
      // 如果是表情图片
      if (element.tagName === 'IMG' && element.classList.contains('input-face-emoji')) {
        const faceId = element.getAttribute('data-face-id');
        if (faceId) {
          result += `[CQ:face,id=${faceId}]`;
        }
      } else if (element.tagName === 'SPAN' && element.classList.contains('input-mention')) {
        // 如果是 @ 元素，只提取 CQ 码，不提取文本内容
        const userId = element.getAttribute('data-user-id');
        if (userId) {
          result += `[CQ:at,qq=${userId}]`;
        }
      } else if (element.tagName === 'BR') {
        // 换行符
        result += '\n';
      } else if (element.tagName === 'DIV' && element !== editor) {
        // 嵌套的 div，递归处理
        const divContent = extractContentFromDiv(element);
        result += divContent;
        if (divContent && !divContent.endsWith('\n')) {
          result += '\n';
        }
      }
    }
  }
  
  return result.trim();
};

// 从 div 元素提取内容（辅助函数）
const extractContentFromDiv = (div: HTMLElement): string => {
  let result = '';
  for (const child of Array.from(div.childNodes)) {
    if (child.nodeType === Node.TEXT_NODE) {
      result += (child.textContent || '').replace(/\u200B/g, '');
    } else if (child.nodeType === Node.ELEMENT_NODE) {
      const element = child as HTMLElement;
      if (element.tagName === 'IMG' && element.classList.contains('input-face-emoji')) {
        const faceId = element.getAttribute('data-face-id');
        if (faceId) {
          result += `[CQ:face,id=${faceId}]`;
        }
      } else if (element.tagName === 'SPAN' && element.classList.contains('input-mention')) {
        // 如果是 @ 元素
        const userId = element.getAttribute('data-user-id');
        if (userId) {
          result += `[CQ:at,qq=${userId}]`;
        }
      } else if (element.tagName === 'BR') {
        result += '\n';
      } else if (element.tagName === 'DIV') {
        const divContent = extractContentFromDiv(element);
        result += divContent;
        if (divContent && !divContent.endsWith('\n')) {
          result += '\n';
        }
      } else {
        result += element.textContent || '';
      }
    }
  }
  return result;
};

// 处理编辑器输入事件
const handleEditorInput = () => {
  console.log('[handleEditorInput] 触发，chatType:', props.chatType);
  
  // 检查是否在群聊
  if (props.chatType !== 'group' || !inputEditorRef.value) {
    console.log('[handleEditorInput] 不是群聊或编辑器不存在，返回');
    return;
  }
  
  // 获取光标位置和之前的文本，检查是否输入了 @
  const selection = window.getSelection();
  if (!selection || selection.rangeCount === 0) {
    console.log('[handleEditorInput] 没有选区');
    return;
  }
  
  const range = selection.getRangeAt(0);
  const container = range.startContainer;
  
  console.log('[handleEditorInput] 容器节点类型:', container.nodeType, 'TEXT_NODE=', Node.TEXT_NODE);
  
  // 如果是文本节点
  if (container.nodeType === Node.TEXT_NODE) {
    const text = container.textContent || '';
    const cursorPos = range.startOffset;
    
    console.log('[handleEditorInput] 文本内容:', text, '光标位置:', cursorPos);
    
    // 查找光标前最近的 @ 符号位置
    const textBeforeCursor = text.substring(0, cursorPos);
    const lastAtIndex = textBeforeCursor.lastIndexOf('@');
    
    console.log('[handleEditorInput] 光标前文本:', textBeforeCursor, '最后的@位置:', lastAtIndex);
    
    if (lastAtIndex !== -1) {
      // 检查 @ 之后是否只有字母、数字或空
      const textAfterAt = textBeforeCursor.substring(lastAtIndex + 1);
      
      console.log('[handleEditorInput] @ 后面的文本:', textAfterAt);
      
      // 如果 @ 后面紧跟空格或已经是 CQ 码格式，不显示选择器
      if (textAfterAt.length === 0 || /^[a-zA-Z0-9\u4e00-\u9fa5]*$/.test(textAfterAt)) {
        console.log('[handleEditorInput] 显示 @ 选择器');
        // 显示 @ 选择器
        showMentionPicker.value = true;
        mentionSearchText.value = textAfterAt;
        selectedMentionIndex.value = 0; // 重置选中索引
        
        // 计算选择器位置（在输入框上方，使用固定位置）
        // 选择器高度约 300px，向上偏移
        try {
          const editorRect = inputEditorRef.value!.getBoundingClientRect();
          // 固定在输入框上方，留出一些间距
          mentionPickerPosition.value = {
            bottom: editorRect.height + 10, // 在输入框上方 10px
            left: 10, // 左侧留 10px 边距
          };
        } catch (e) {
          // 如果获取位置失败，使用默认位置
          mentionPickerPosition.value = { bottom: 60, left: 10 };
        }
        
        return;
      }
    }
  }
  
  // 如果没有检测到 @，关闭选择器
  showMentionPicker.value = false;
  mentionSearchText.value = '';
  selectedMentionIndex.value = 0; // 重置选中索引
};

// 过滤后的群成员列表（用于 @ 选择）
const filteredMemberList = computed(() => {
  if (props.chatType !== 'group' || !props.chatId) {
    return [];
  }
  
  const members = getGroupMembers(props.chatId);
  const searchLower = mentionSearchText.value.toLowerCase();
  
  if (!searchLower) {
    // 如果没有搜索词，返回所有成员（最多显示 10 个）
    return members.slice(0, 10);
  }
  
  // 根据搜索词过滤
  return members.filter(member => {
    const displayName = (member.card || member.nickname || '').toLowerCase();
    const userId = String(member.userId);
    return displayName.includes(searchLower) || userId.includes(searchLower);
  }).slice(0, 10);
});

// 选择要 @ 的成员
const selectMention = (userId: number, displayName: string) => {
  console.log('[selectMention] 选择成员:', userId, displayName);
  
  if (!inputEditorRef.value || !props.chatId || !props.chatType) {
    console.log('[selectMention] 编辑器或聊天信息不存在');
    return;
  }
  
  const selection = window.getSelection();
  if (!selection || selection.rangeCount === 0) {
    console.log('[selectMention] 没有选区');
    return;
  }
  
  console.log('[selectMention] 创建 @ 元素');
  // 创建 @ 元素（类似表情元素）
  const mentionSpan = document.createElement('span');
  mentionSpan.className = 'input-mention';
  mentionSpan.setAttribute('data-user-id', String(userId));
  mentionSpan.setAttribute('data-display-name', displayName);
  mentionSpan.setAttribute('contenteditable', 'false');
  mentionSpan.textContent = `@${displayName}`;
  mentionSpan.style.color = '#0088cc';
  mentionSpan.style.backgroundColor = 'rgba(0, 136, 204, 0.1)';
  mentionSpan.style.padding = '2px 6px';
  mentionSpan.style.borderRadius = '4px';
  mentionSpan.style.display = 'inline-block';
  mentionSpan.style.margin = '0 2px';
  mentionSpan.style.cursor = 'pointer';
  
  console.log('[selectMention] 插入 @ 元素');
  
  // 获取编辑器的完整内容
  const editorHtml = inputEditorRef.value.innerHTML;
  console.log('[selectMention] 编辑器原始 HTML:', editorHtml);
  
  // 找到最后一个 @ 的位置
  const lastAtIndex = editorHtml.lastIndexOf('@');
  
  if (lastAtIndex !== -1) {
    console.log('[selectMention] 找到 @ 位置:', lastAtIndex);
    
    // 删除 @ 到最后的所有内容（包括 @ 和后面的搜索文本）
    const beforeAt = editorHtml.substring(0, lastAtIndex);
    
    // 创建临时容器来构建新 HTML
    const tempDiv = document.createElement('div');
    tempDiv.innerHTML = beforeAt;
    
    // 添加 @ 元素
    tempDiv.appendChild(mentionSpan);
    
    // 添加空格
    tempDiv.appendChild(document.createTextNode('\u00A0'));
    
    // 更新编辑器内容
    inputEditorRef.value.innerHTML = tempDiv.innerHTML;
    
    console.log('[selectMention] 更新后 HTML:', inputEditorRef.value.innerHTML);
    
    // 将光标移到最后
    const range = document.createRange();
    const sel = window.getSelection();
    range.selectNodeContents(inputEditorRef.value);
    range.collapse(false); // 折叠到末尾
    sel?.removeAllRanges();
    sel?.addRange(range);
  } else {
    console.log('[selectMention] 未找到 @，直接追加');
    // 没找到 @，直接追加
    inputEditorRef.value.appendChild(mentionSpan);
    inputEditorRef.value.appendChild(document.createTextNode('\u00A0'));
    
    // 将光标移到最后
    const range = document.createRange();
    const sel = window.getSelection();
    range.selectNodeContents(inputEditorRef.value);
    range.collapse(false);
    sel?.removeAllRanges();
    sel?.addRange(range);
  }
  
  console.log('[selectMention] @ 元素插入完成，编辑器内容:', inputEditorRef.value.innerHTML);
  
  // 保存 @的用户（用于发送时构造 CQ 码）
  addMentionedUser(props.chatType, props.chatId, userId, displayName);
  
  // 关闭选择器
  showMentionPicker.value = false;
  mentionSearchText.value = '';
  
  // 聚焦编辑器
  inputEditorRef.value.focus();
};

// 点击外部关闭 @ 选择器
const handleClickOutsideMention = (event: MouseEvent) => {
  if (mentionPickerRef.value && !mentionPickerRef.value.contains(event.target as Node)) {
    const target = event.target as HTMLElement;
    // 如果点击的不是编辑器，关闭选择器
    if (!target.closest('.rich-input-editor')) {
      showMentionPicker.value = false;
      mentionSearchText.value = '';
    }
  }
};

// 处理编辑器粘贴事件
const handleEditorPaste = async (event: ClipboardEvent) => {
  // 如果正在发送，忽略粘贴
  if (sending.value) {
    event.preventDefault();
    return;
  }
  
  // 检查是否有图片
  const items = event.clipboardData?.items;
  if (items) {
    for (let i = 0; i < items.length; i++) {
      const item = items[i];
      if (item.type.indexOf('image') !== -1) {
        event.preventDefault();
        const file = item.getAsFile();
        if (file) {
          // 处理图片文件
          const preview = URL.createObjectURL(file);
          selectedImages.value.push({ file, preview });
        }
        return;
      }
    }
  }
  
  // 允许默认粘贴行为（文本）
  // 但需要清理粘贴的内容，移除格式
  event.preventDefault();
  const text = event.clipboardData?.getData('text/plain') || '';
  if (text) {
    const selection = window.getSelection();
    if (selection && selection.rangeCount > 0) {
      const range = selection.getRangeAt(0);
      range.deleteContents();
      const textNode = document.createTextNode(text);
      range.insertNode(textNode);
      range.setStartAfter(textNode);
      range.collapse(true);
      selection.removeAllRanges();
      selection.addRange(range);
    }
  }
};

// 处理编辑器键盘事件
const handleEditorKeyDown = (event: KeyboardEvent) => {
  // 如果 @ 选择器正在显示，处理上下箭头和回车键
  if (showMentionPicker.value && filteredMemberList.value.length > 0) {
    if (event.key === 'ArrowDown') {
      event.preventDefault();
      selectedMentionIndex.value = (selectedMentionIndex.value + 1) % filteredMemberList.value.length;
      // 滚动到可见区域
      nextTick(() => {
        const selectedElement = mentionPickerRef.value?.querySelector('.mention-item.selected');
        if (selectedElement) {
          selectedElement.scrollIntoView({ block: 'nearest', behavior: 'smooth' });
        }
      });
      return;
    } else if (event.key === 'ArrowUp') {
      event.preventDefault();
      selectedMentionIndex.value = selectedMentionIndex.value === 0 
        ? filteredMemberList.value.length - 1 
        : selectedMentionIndex.value - 1;
      // 滚动到可见区域
      nextTick(() => {
        const selectedElement = mentionPickerRef.value?.querySelector('.mention-item.selected');
        if (selectedElement) {
          selectedElement.scrollIntoView({ block: 'nearest', behavior: 'smooth' });
        }
      });
      return;
    } else if (event.key === 'Enter' && !event.shiftKey) {
      event.preventDefault();
      const selectedMember = filteredMemberList.value[selectedMentionIndex.value];
      if (selectedMember) {
        selectMention(selectedMember.userId, selectedMember.card || selectedMember.nickname);
      }
      return;
    } else if (event.key === 'Escape') {
      event.preventDefault();
      showMentionPicker.value = false;
      selectedMentionIndex.value = 0;
      return;
    }
  }
  
  // Enter 键发送消息
  if (event.key === 'Enter' && !event.shiftKey) {
    // 检查输入法状态
    if (isComposing.value) {
      return;
    }
    
    // 检查输入法结束时间
    const now = Date.now();
    if (now - compositionEndTime.value < 100) {
      return;
    }
    
    event.preventDefault();
    sendMessage();
    return;
  }
  
  // Shift+Enter 换行（允许默认行为）
};

// 点击外部关闭表情选择器
const handleClickOutside = (event: MouseEvent) => {
  if (facePickerRef.value && !facePickerRef.value.contains(event.target as Node)) {
    const target = event.target as HTMLElement;
    // 如果点击的不是表情按钮，关闭选择器
    if (!target.closest('.face-button')) {
      showFacePicker.value = false;
    }
  }
};

// 打开图片查看器窗口
const openImageViewer = async (imageUrl: string) => {
  console.log('点击图片，准备打开查看器:', imageUrl);
  
  try {
    // 检查窗口是否已存在
    const windowId = 'image-viewer';
    const existingWindow = await WebviewWindow.getByLabel(windowId);
    
    if (existingWindow) {
      // 如果窗口已存在，关闭它
      try {
        await existingWindow.close();
      } catch (e) {
        // 忽略关闭错误
        console.warn('关闭已存在的窗口失败:', e);
      }
    }
    
    // 获取当前窗口的位置和大小
    const currentWindow = getCurrentWindow();
    const position = await currentWindow.innerPosition();
    const size = await currentWindow.innerSize();
    
    // 获取屏幕尺寸，创建比当前窗口更大的窗口
    const monitor = await currentMonitor();
    const screenWidth = monitor?.size.width || 1920;
    const screenHeight = monitor?.size.height || 1080;
    
    // 创建新窗口，比当前窗口大一些，但不超过屏幕的 90%
    const newWidth = Math.min(Math.max(size.width * 1.5, 800), screenWidth * 0.9);
    const newHeight = Math.min(Math.max(size.height * 1.5, 600), screenHeight * 0.9);
    
    // 构建 URL（开发环境使用 localhost，生产环境使用相对路径）
    const isDev = import.meta.env.DEV;
    const baseUrl = isDev ? 'http://localhost:1420' : '';
    const viewerUrl = `${baseUrl}/src/pages/image-viewer.html?url=${encodeURIComponent(imageUrl)}`;
    
    console.log('创建图片查看器窗口:', { viewerUrl, newWidth, newHeight });
    
    new WebviewWindow(windowId, {
      url: viewerUrl,
      title: '图片查看器',
      width: newWidth,
      height: newHeight,
      x: position.x + (size.width - newWidth) / 2,
      y: position.y + (size.height - newHeight) / 2,
      resizable: true,
      minimizable: true,
      maximizable: true,
      closable: true,
      decorations: true,
      alwaysOnTop: false,
      skipTaskbar: false,
      focus: true,
      transparent: false,
      center: false,
      visible: false, // 初始隐藏，等调整好大小后再显示
    });
    
    console.log('图片查看器窗口已创建');
  } catch (error) {
    console.error('打开图片查看器失败:', error);
    // 如果创建窗口失败，使用浏览器方式打开
    try {
      window.open(imageUrl, '_blank');
    } catch (e) {
      console.error('浏览器打开也失败:', e);
    }
  }
};


// 发送消息
const sendMessage = async () => {
  // 从富文本编辑器提取内容
  const editorContent = extractContentFromEditor();
  const hasText = editorContent.trim().length > 0;
  const hasImages = selectedImages.value.length > 0;
  
  if ((!hasText && !hasImages) || !props.chatId || !props.chatType || sending.value || !props.selfId) {
    return;
  }

  // 构建消息数组（根据 NapCat API 文档格式）
  const messageArray: Array<{ type: string; data: Record<string, any> }> = [];
  
  // 如果是回复消息，在消息开头添加 reply 段
  if (replyToMessage.value && replyToMessage.value.message_id) {
    messageArray.push({
      type: 'reply',
      data: {
        id: replyToMessage.value.message_id.toString(),
      },
    });
  }
  
  // 解析输入框中的内容，将 CQ 码转换为数组元素
  // @ 已经在 extractContentFromEditor 中转换为 [CQ:at,qq=userId] 了
  if (hasText) {
    const textContent = editorContent.trim();
    const segments = parseCQCode(textContent);
    
    // 将解析后的段转换为消息数组
    for (const segment of segments) {
      if (segment.type === 'text') {
        // 文本段
        if (segment.text && segment.text.trim()) {
          messageArray.push({
            type: 'text',
            data: {
              text: segment.text,
            },
          });
        }
      } else if (segment.type === 'face') {
        // 表情段
        const faceId = segment.data.id;
        if (faceId) {
          messageArray.push({
            type: 'face',
            data: {
              id: faceId,
            },
          });
        }
      } else if (segment.type === 'at') {
        // @ 段
        const qq = segment.data.qq;
        if (qq) {
          messageArray.push({
            type: 'at',
            data: {
              qq: qq,
            },
          });
        }
      } else if (segment.type === 'image') {
        // 图片段（从输入框中的 CQ 码解析出来的，这种情况较少见）
        const file = segment.data.file;
        if (file) {
          messageArray.push({
            type: 'image',
            data: {
              file: file,
            },
          });
        }
      }
      // 其他类型的 CQ 码可以在这里处理
    }
  }
  
  // 检查消息是否包含图片
  let hasImage = false;
  
  // 添加选中的图片（base64 编码）
  for (const image of selectedImages.value) {
    hasImage = true;
    try {
      const { base64 } = await fileToBase64(image.file);
      // 根据 NapCat API 文档，使用数组格式
      messageArray.push({
        type: 'image',
        data: {
          file: `base64://${base64}`,
          summary: '[图片]',
        },
      });
    } catch (error) {
      console.error('转换图片为 base64 失败:', error);
      alert(`转换图片 ${image.file.name} 失败`);
      return;
    }
  }
  
  // 检查消息数组中是否已有图片（从 CQ 码解析出来的）
  if (!hasImage) {
    hasImage = messageArray.some(item => item.type === 'image');
  }
  
  // 为了显示，也构建一个 CQ 码格式的字符串（用于本地显示）
  const messageText = messageArray.map(item => {
    if (item.type === 'text') {
      return item.data.text;
    } else if (item.type === 'image') {
      return `[CQ:image,file=${item.data.file}]`;
    } else if (item.type === 'face') {
      return `[CQ:face,id=${item.data.id}]`;
    } else if (item.type === 'at') {
      return `[CQ:at,qq=${item.data.qq}]`;
    } else if (item.type === 'reply') {
      return `[CQ:reply,id=${item.data.id}]`;
    }
    return '';
  }).join('');
  const now = Math.floor(Date.now() / 1000);
  
  // 生成本地消息 ID
  const localMessageId = generateUUID();
  
  // 创建本地消息记录（立即显示）
  const localMessage: OneBotMessage = {
    localMessageId,
    time: now,
    self_id: props.selfId,
    post_type: 'message_sent',
    message_type: props.chatType,
    sub_type: undefined,
    message_id: undefined,
    // 对于私聊，user_id 应该是接收者的 ID（chatId）
    // 对于群聊，user_id 应该是自己的 ID（selfId），group_id 是群组 ID
    user_id: props.chatType === 'private' ? props.chatId : props.selfId,
    group_id: props.chatType === 'group' ? props.chatId : undefined,
    message: messageText,
    raw_message: messageText,
    sender: undefined,
    raw: undefined,
    sendStatus: 'sending', // 初始状态为发送中
  };
  
  // 立即添加到消息列表（乐观更新）
  messages.value.push(localMessage);
  
  // 保存到数据库
  try {
    await saveMessage(localMessage, props.selfId);
  } catch (error) {
    console.error('保存消息失败:', error);
  }
  
  // 如果滚动条距离底部小于150px，自动滚动到底部（等待 DOM 更新）
  nextTick(() => {
    if (shouldAutoScroll()) {
      scrollToBottom();
    }
  });
  
  // 清空输入框和选中的图片
  if (inputEditorRef.value) {
    inputEditorRef.value.innerHTML = '';
  }
  // 清理预览 URL
  selectedImages.value.forEach(img => URL.revokeObjectURL(img.preview));
  selectedImages.value = [];
  
  // 清空回复状态
  replyToMessage.value = null;
  
  // 同时清空聊天输入状态
  if (props.chatId && props.chatType) {
    clearChatInputState(props.chatType, props.chatId);
  }
  
  sending.value = true;
  try {
    if (props.chatType === 'private') {
      await runbotService.sendPrivateMessage(props.chatId, messageArray, localMessageId, true, hasImage);
    } else if (props.chatType === 'group') {
      await runbotService.sendGroupMessage(props.chatId, messageArray, localMessageId, true, hasImage);
    }
  } catch (error) {
    console.error('发送消息失败:', error);
    alert(`发送消息失败: ${error}`);
    // 发送失败，更新消息状态为失败
    const index = messages.value.findIndex(msg => 
      msg.localMessageId === localMessage.localMessageId
    );
    if (index !== -1) {
      messages.value[index].sendStatus = 'failed';
    }
  } finally {
    sending.value = false;
  }
};

// 格式化时间
const formatTime = (timestamp: number): string => {
  const date = new Date(timestamp * 1000);
  return date.toLocaleTimeString('zh-CN', { hour: '2-digit', minute: '2-digit' });
};

// 解析消息内容为 CQ 段
const parseMessage = (msg: OneBotMessage): CQSegment[] => {
  const content = msg.message || msg.raw_message || '';
  return parseCQCode(content);
};

// 根据 message_id 获取消息
const getMessageById = (messageId: number): OneBotMessage | undefined => {
  return messages.value.find(m => m.message_id === messageId);
};

// 获取被回复的消息（从 CQ 段中提取）
const getReplyMessage = (segments: CQSegment[]): OneBotMessage | null => {
  const replySegment = segments.find(s => s.type === 'reply');
  if (!replySegment || !replySegment.data.id) {
    return null;
  }
  
  const replyMessageId = parseInt(replySegment.data.id);
  if (isNaN(replyMessageId)) {
    return null;
  }
  
  return getMessageById(replyMessageId) || null;
};

// 检查消息是否只包含图片/表情（没有文本）
// 判断是否为单个图片（没有文本，只有一个图片）
const isSingleImage = (segments: CQSegment[]): boolean => {
  if (segments.length !== 1) return false;
  const segment = segments[0];
  return segment.type === 'image' && (!segment.text || !segment.text.trim());
};


// 判断是否为只有图片/表情的消息（可能有多个，但没有文本）
const isImageOnlyMessage = (segments: CQSegment[]): boolean => {
  if (segments.length === 0) return false;
  
  // 检查是否只有图片或表情，没有文本
  const hasText = segments.some(s => s.type === 'text' && s.text && s.text.trim());
  const hasImageOrFace = segments.some(s => s.type === 'image' || s.type === 'face');
  
  return !hasText && hasImageOrFace;
};

// 图片缓存映射（URL -> 本地路径）
const imageCache = ref<Map<string, string>>(new Map());

// 图片加载状态（URL -> 是否正在加载）
const imageLoading = ref<Map<string, boolean>>(new Map());

// 图片加载失败标记（URL -> 是否加载失败）
const imageFailed = ref<Map<string, boolean>>(new Map());

// 加载图片到缓存
const loadImage = async (url: string, file?: string) => {
  if (!props.selfId || !url) return;
  
  // 如果已经在缓存中或正在加载，跳过
  if (imageCache.value.has(url) || imageLoading.value.get(url)) {
    return;
  }
  
  // 如果已经加载失败，跳过
  if (imageFailed.value.get(url)) {
    return;
  }
  
  imageLoading.value.set(url, true);
  
  try {
    // 先检查缓存
    let cachedPath = await checkImageCache(url, props.selfId);
    
    // 如果没有缓存，下载（传递 file 参数用于 URL 过期时重新获取）
    if (!cachedPath) {
      cachedPath = await downloadImage(url, props.selfId, file);
    }
    
    if (cachedPath) {
      imageCache.value.set(url, cachedPath);
      imageFailed.value.set(url, false);
    } else {
      imageFailed.value.set(url, true);
    }
  } catch (error) {
    console.error('加载图片失败:', error);
    imageFailed.value.set(url, true);
  } finally {
    imageLoading.value.set(url, false);
  }
};

// Intersection Observer 用于图片懒加载
let imageObserver: IntersectionObserver | null = null;

// 初始化图片懒加载观察器
const initImageObserver = () => {
  if (typeof IntersectionObserver === 'undefined') {
    return; // 浏览器不支持 IntersectionObserver
  }
  
  imageObserver = new IntersectionObserver(
      (entries) => {
        for (const entry of entries) {
          if (entry.isIntersecting) {
            const element = entry.target as HTMLElement;
            const url = element.dataset.imageUrl;
            const file = element.dataset.imageFile;
            if (url && !imageCache.value.has(url) && !imageLoading.value.get(url)) {
              loadImage(url, file);
              // 加载后停止观察
              imageObserver?.unobserve(element);
            }
          }
        }
      },
    {
      root: messagesContainer.value,
      rootMargin: '200px', // 提前 200px 开始加载
      threshold: 0.01,
    }
  );
};

// 观察图片占位符元素（在 DOM 更新后）
const observeImagePlaceholders = () => {
  if (!imageObserver || !messagesContainer.value) return;
  
  nextTick(() => {
    const placeholders = messagesContainer.value?.querySelectorAll('[data-image-url]');
    placeholders?.forEach((placeholder) => {
      const url = (placeholder as HTMLElement).dataset.imageUrl;
      if (url && !imageCache.value.has(url) && !imageLoading.value.get(url) && !imageFailed.value.get(url)) {
        imageObserver?.observe(placeholder);
      }
    });
  });
};

// 监听消息变化，观察新的图片占位符
watch(() => filteredMessages.value, () => {
  observeImagePlaceholders();
}, { deep: true });

// 渲染消息内容（支持 CQ 码）
const renderMessage = (segments: CQSegment[]): any[] => {
  return segments.map((segment, index) => {
    if (segment.type === 'text' && segment.text) {
      return {
        type: 'text',
        content: segment.text,
        key: `text-${index}`,
      };
    } else if (segment.type === 'image') {
      // 支持 base64:// 格式的图片
      const file = segment.data.file || '';
      const url = segment.data.url || '';
      const subType = segment.data.sub_type || '0';
      const summary = segment.data.summary || '';
      
      // 解码 HTML 实体
      const decodedSummary = summary
        .replace(/&#91;/g, '[')
        .replace(/&#93;/g, ']')
        .replace(/&amp;/g, '&')
        .replace(/&lt;/g, '<')
        .replace(/&gt;/g, '>')
        .replace(/&quot;/g, '"')
        .replace(/&#39;/g, "'");
      
      // 如果是 base64:// 格式，直接使用 file 参数
      if (file.startsWith('base64://')) {
        // base64 图片，直接显示（不需要下载）
        const base64Data = file.substring(9); // 移除 "base64://" 前缀
        // 尝试检测图片类型（通过 base64 数据的前几个字节）
        // 默认使用 image/png，但可以根据需要检测
        let mimeType = 'image/png';
        // 简单的 MIME 类型检测（可选，如果 OneBot 服务器提供了类型信息会更好）
        return {
          type: 'image',
          url: `data:${mimeType};base64,${base64Data}`, // 使用 data URI
          key: `image-${index}-base64-${base64Data.substring(0, 20)}`,
          subType,
          summary: decodedSummary,
        };
      }
      
      // 普通 URL 图片
      const imageUrl = url || file;
      return {
        type: 'image',
        url: imageUrl,
        file: file, // 保存 file 参数，用于 URL 过期时重新获取
        key: `image-${index}-${imageUrl}`,
        subType,
        summary: decodedSummary,
      };
    } else if (segment.type === 'face') {
      return {
        type: 'face',
        id: segment.data.id || '',
        key: `face-${index}`,
      };
    } else if (segment.type === 'at') {
      const userId = segment.data.qq || '';
      let displayName = `@${userId}`;
      
      // 如果是群聊，尝试从群成员缓存中获取昵称
      if (props.chatType === 'group' && props.chatId && userId) {
        const memberName = getGroupMemberDisplayName(props.chatId, parseInt(userId));
        if (memberName !== `用户 ${userId}`) {
          displayName = `@${memberName}`;
        }
      }
      
      return {
        type: 'at',
        qq: userId,
        displayName,
        key: `at-${index}`,
      };
    } else if (segment.type === 'reply') {
      // reply 类型不渲染，在外层处理
      return null;
    } else if (segment.type === 'forward') {
      // 合并转发消息
      return {
        type: 'forward',
        id: segment.data.id || '',
        key: `forward-${index}`,
      };
    } else {
      // 未知类型，显示原始文本
      return {
        type: 'text',
        content: `[CQ:${segment.type}]`,
        key: `unknown-${index}`,
      };
    }
  }).filter(item => item !== null); // 过滤掉 null 项
};

// 渲染被回复的消息内容（简化版本，不展示回复的回复）
const renderReplyMessage = (msg: OneBotMessage): any[] => {
  const segments = parseMessage(msg);
  
  return segments.map((segment, index) => {
    if (segment.type === 'text' && segment.text) {
      return {
        type: 'text',
        content: segment.text,
        key: `reply-text-${index}`,
      };
    } else if (segment.type === 'image') {
      return {
        type: 'text',
        content: '[图片]',
        key: `reply-image-${index}`,
      };
    } else if (segment.type === 'face') {
      const faceText = getFaceDisplayText(segment.data.id || '');
      return {
        type: 'text',
        content: faceText,
        key: `reply-face-${index}`,
      };
    } else if (segment.type === 'at') {
      const userId = segment.data.qq || '';
      let displayName = `@${userId}`;
      
      // 如果是群聊，尝试从群成员缓存中获取昵称
      if (props.chatType === 'group' && props.chatId && userId) {
        const memberName = getGroupMemberDisplayName(props.chatId, parseInt(userId));
        if (memberName !== `用户 ${userId}`) {
          displayName = `@${memberName}`;
        }
      }
      
      return {
        type: 'text',
        content: displayName,
        key: `reply-at-${index}`,
      };
    } else if (segment.type === 'reply') {
      // 回复的回复，显示为固定文本
      return {
        type: 'text',
        content: '[回复消息]',
        key: `reply-reply-${index}`,
      };
    } else {
      return {
        type: 'text',
        content: `[CQ:${segment.type}]`,
        key: `reply-unknown-${index}`,
      };
    }
  });
};

// 获取消息的纯文本内容（用于回复预览）
const getMessagePreviewText = (segments: CQSegment[]): string => {
  return segments.map(segment => {
    if (segment.type === 'text' && segment.text) {
      return segment.text;
    } else if (segment.type === 'image') {
      return '[图片]';
    } else if (segment.type === 'face') {
      return getFaceDisplayText(segment.data.id || '');
    } else if (segment.type === 'at') {
      const userId = segment.data.qq || '';
      if (props.chatType === 'group' && props.chatId && userId) {
        const memberName = getGroupMemberDisplayName(props.chatId, parseInt(userId));
        if (memberName !== `用户 ${userId}`) {
          return `@${memberName}`;
        }
      }
      return `@${userId}`;
    } else if (segment.type === 'reply') {
      return '[回复消息]';
    } else {
      return `[CQ:${segment.type}]`;
    }
  }).join('');
};

// 获取发送者名称
const getSenderName = (msg: OneBotMessage): string => {
  // 如果是自己发送的消息，显示"我"
  if (msg.post_type === 'message_sent') {
    return '我';
  }
  
  if (msg.sender) {
    return msg.sender.nickname || msg.sender.card || `用户 ${msg.user_id}`;
  }
  return `用户 ${msg.user_id || '未知'}`;
};

// 检查是否应该自动滚动（距离底部小于250px）
const shouldAutoScroll = (): boolean => {
  if (!messagesContainer.value) return false;
  const container = messagesContainer.value;
  const scrollTop = container.scrollTop;
  const scrollHeight = container.scrollHeight;
  const clientHeight = container.clientHeight;
  const distanceFromBottom = scrollHeight - scrollTop - clientHeight;
  return distanceFromBottom <= 250;
};

// 滚动到底部
const scrollToBottom = () => {
  nextTick(() => {
    // 使用 setTimeout 确保 DOM 完全更新
    setTimeout(() => {
      if (messagesContainer.value) {
        messagesContainer.value.scrollTop = messagesContainer.value.scrollHeight;
      }
    }, 50);
  });
};

// 恢复当前聊天的输入状态
const restoreInputState = () => {
  if (!props.chatId || !props.chatType) {
    // 如果没有选中聊天，清空输入框
    if (inputEditorRef.value) {
      inputEditorRef.value.innerHTML = '';
    }
    selectedImages.value.forEach(img => URL.revokeObjectURL(img.preview));
    selectedImages.value = [];
    console.log('[restoreInputState] 清空输入框（无聊天选中）');
    return;
  }
  
  const state = getChatInputState(props.chatType, props.chatId);
  
  console.log('[restoreInputState] 恢复输入状态:', props.chatType, props.chatId, '内容长度:', state.editorHtml.length);
  
  // 恢复编辑器内容
  if (inputEditorRef.value) {
    inputEditorRef.value.innerHTML = state.editorHtml;
  }
  
  // 恢复选中的图片
  selectedImages.value = state.selectedImages;
};

// 监听聊天变化
watch(() => [props.chatId, props.chatType], (_newVal, oldVal) => {
  // 保存旧聊天的输入状态
  if (oldVal && oldVal[0] && oldVal[1] && inputEditorRef.value) {
    const oldChatType = oldVal[1] as 'private' | 'group';
    const oldChatId = oldVal[0] as number;
    const editorHtml = inputEditorRef.value.innerHTML || '';
    
    console.log('[watch] 保存旧聊天输入状态:', oldChatType, oldChatId, '内容长度:', editorHtml.length);
    
    updateChatInputState(oldChatType, oldChatId, {
      editorHtml,
      selectedImages: selectedImages.value,
    });
  }
  
  // 重置头像加载失败状态
  chatAvatarFailed.value = false;
  
  // 清空回复状态
  replyToMessage.value = null;
  
  if (props.chatId && props.chatType) {
    loadChatMessages();
    // 恢复新聊天的输入状态
    nextTick(() => {
      restoreInputState();
    });
  } else {
    messages.value = [];
    // 清空输入框
    nextTick(() => {
      restoreInputState();
    });
  }
}, { immediate: true });

// 监听新消息（从父组件传入）
const addMessage = (msg: OneBotMessage) => {
  // 检查是否属于当前聊天（包括发送的消息）
  // 对于发送的消息（message_sent），需要特殊处理：
  // - 私聊：message_type 是 'private' 且 user_id 是接收者 ID（chatId）
  // - 群聊：message_type 是 'group' 且 group_id 是群组 ID（chatId）
  // 对于接收的消息（message），正常判断
  const isCurrentChat = msg.post_type === 'message_sent' 
    ? ((props.chatType === 'private' && msg.message_type === 'private' && msg.user_id === props.chatId) ||
       (props.chatType === 'group' && msg.message_type === 'group' && msg.group_id === props.chatId))
    : ((props.chatType === 'private' && msg.message_type === 'private' && msg.user_id === props.chatId) ||
       (props.chatType === 'group' && msg.message_type === 'group' && msg.group_id === props.chatId));
  
  if (isCurrentChat && (msg.post_type === 'message' || msg.post_type === 'message_sent')) {
    // 如果消息没有 localMessageId，生成一个
    if (!msg.localMessageId) {
      msg.localMessageId = generateUUID();
    }
    
    // 检查是否已存在（使用 localMessageId 检查）
    const exists = messages.value.some(existing => 
      existing.localMessageId === msg.localMessageId && 
      existing.localMessageId !== undefined
    );
    
    if (!exists) {
      messages.value.push(msg);
      // 注意：不在这里保存到数据库，因为 MainView.vue 已经保存了
      // 这里只负责显示
      
      // 如果滚动条距离底部小于150px，自动滚动到底部（等待 DOM 更新）
      nextTick(() => {
        if (shouldAutoScroll()) {
          scrollToBottom();
        }
        // 观察新消息中的图片
        observeImagePlaceholders();
      });
    }
  }
};

// 初始化图片观察器
onMounted(async () => {
  nextTick(() => {
    initImageObserver();
    observeImagePlaceholders();
  });
  
  // 监听点击外部关闭表情选择器和 @ 选择器
  document.addEventListener('click', handleClickOutside);
  document.addEventListener('click', handleClickOutsideMention);
  
  // 监听消息发送成功事件
  try {
    const unlistenSent = await listen<{ local_message_id: string; message_id: number }>('message-sent', (event) => {
      const { local_message_id, message_id } = event.payload;
      
      // 更新对应消息的状态和 message_id
      const messageIndex = messages.value.findIndex(msg => msg.localMessageId === local_message_id);
      if (messageIndex !== -1) {
        messages.value[messageIndex].sendStatus = 'sent';
        messages.value[messageIndex].message_id = message_id;
      }
    });
    
    // 监听消息内容更新事件（用于更新图片消息的 base64 为正常 URL）
    const unlistenUpdated = await listen<{ local_message_id: string; message_id: number; message: string; raw_message: string }>('message-updated', (event) => {
      const { local_message_id, message, raw_message } = event.payload;
      
      // 更新对应消息的内容
      const messageIndex = messages.value.findIndex(msg => msg.localMessageId === local_message_id);
      if (messageIndex !== -1) {
        messages.value[messageIndex].message = message;
        messages.value[messageIndex].raw_message = raw_message;
      }
    });
    
    // 在组件卸载时取消监听
    onUnmounted(() => {
      unlistenSent();
      unlistenUpdated();
    });
  } catch (error) {
    console.error('监听消息事件失败:', error);
  }
});

// 清理图片观察器
onUnmounted(() => {
  if (imageObserver) {
    imageObserver.disconnect();
    imageObserver = null;
  }
  
  // 移除点击外部监听
  document.removeEventListener('click', handleClickOutside);
  document.removeEventListener('click', handleClickOutsideMention);
});

// 显示右键菜单
const showMessageContextMenu = (event: MouseEvent, msg: OneBotMessage) => {
  console.log('[ChatArea] 右键点击消息:', msg);
  
  event.preventDefault();
  contextMenuMessage.value = msg;
  
  // 计算菜单位置，确保不超出视窗
  const menuWidth = 120; // 菜单宽度
  const menuHeight = 80; // 菜单高度（增加了回复选项）
  const windowWidth = window.innerWidth;
  const windowHeight = window.innerHeight;
  
  let x = event.clientX;
  let y = event.clientY;
  
  // 如果右侧超出，调整到左侧
  if (x + menuWidth > windowWidth) {
    x = windowWidth - menuWidth - 10;
  }
  
  // 如果底部超出，调整到上方
  if (y + menuHeight > windowHeight) {
    y = windowHeight - menuHeight - 10;
  }
  
  contextMenuX.value = x;
  contextMenuY.value = y;
  showContextMenu.value = true;
};

// 关闭右键菜单
const closeContextMenu = () => {
  showContextMenu.value = false;
  contextMenuMessage.value = null;
};

// 判断消息是否可以撤回
const canRecallMessage = (msg: OneBotMessage | null): boolean => {
  if (!msg) return false;
  
  // 只有自己发送的消息且未被撤回才能撤回
  if (msg.post_type !== 'message_sent') return false;
  if (msg.recalled) return false;
  
  // 检查消息是否在2分钟内（120秒）
  const now = Math.floor(Date.now() / 1000);
  const messageTime = msg.time;
  const timeDiff = now - messageTime;
  
  return timeDiff <= 120;
};

// 回复消息
const replyMessage = () => {
  if (!contextMenuMessage.value) return;
  
  // 不能回复已撤回的消息
  if (contextMenuMessage.value.recalled) {
    console.warn('[ChatArea] 无法回复已撤回的消息');
    closeContextMenu();
    return;
  }
  
  console.log('[ChatArea] 设置回复消息:', contextMenuMessage.value);
  replyToMessage.value = contextMenuMessage.value;
  closeContextMenu();
  
  // 聚焦到输入框
  nextTick(() => {
    inputEditorRef.value?.focus();
  });
};

// 取消回复
const cancelReply = () => {
  replyToMessage.value = null;
};

// 撤回消息
const recallMessage = async () => {
  if (!contextMenuMessage.value || !contextMenuMessage.value.message_id) {
    console.error('[ChatArea] 无法撤回消息：消息或 message_id 为空');
    closeContextMenu();
    return;
  }
  
  const messageId = contextMenuMessage.value.message_id;
  console.log('[ChatArea] 撤回消息:', messageId);
  
  try {
    // 调用 API 撤回消息
    await runbotService.deleteMessage(messageId);
    console.log('[ChatArea] 消息撤回成功');
    closeContextMenu();
  } catch (error) {
    console.error('[ChatArea] 撤回消息失败:', error);
    closeContextMenu();
  }
};

// 打开合并转发消息查看器
const openForwardMessage = async (forwardId: string) => {
  console.log('[ChatArea] 打开合并转发消息:', forwardId);
  
  try {
    // 使用固定的窗口 ID（类似图片查看器）
    const windowId = 'forward-viewer';
    
    // 检查窗口是否已存在
    const { WebviewWindow } = await import('@tauri-apps/api/webviewWindow');
    const existingWindow = await WebviewWindow.getByLabel(windowId);
    
    if (existingWindow) {
      // 如果窗口已存在，关闭它
      try {
        await existingWindow.close();
      } catch (e) {
        console.warn('[ChatArea] 关闭已存在的窗口失败:', e);
      }
    }
    
    // 构建 URL（开发环境使用 localhost，生产环境使用相对路径）
    const isDev = import.meta.env.DEV;
    const baseUrl = isDev ? 'http://localhost:1420' : '';
    const viewerUrl = `${baseUrl}/src/pages/forward-viewer.html?id=${forwardId}`;
    
    console.log('[ChatArea] 创建转发消息查看器窗口:', viewerUrl);
    
    // 创建新窗口
    const webview = new WebviewWindow(windowId, {
      url: viewerUrl,
      title: '聊天记录',
      width: 800,
      height: 600,
      center: true,
      visible: true, // 直接显示窗口
    });
    
    // 监听窗口加载完成
    webview.once('tauri://created', () => {
      console.log('[ChatArea] 合并转发查看器窗口已创建');
    });
    
    webview.once('tauri://error', (e) => {
      console.error('[ChatArea] 创建合并转发查看器窗口失败:', e);
      alert('打开窗口失败');
    });
  } catch (error) {
    console.error('[ChatArea] 打开合并转发消息失败:', error);
    alert('打开窗口失败');
  }
};

// 处理消息被撤回
const handleMessageRecalled = async (messageId: number) => {
  console.log('[ChatArea] handleMessageRecalled 被调用, messageId:', messageId);
  console.log('[ChatArea] 当前消息列表:', messages.value.map(m => ({
    message_id: m.message_id,
    localMessageId: m.localMessageId,
    raw_message: m.raw_message,
    recalled: m.recalled
  })));
  
  // 在内存中的消息列表中标记为已撤回
  const msg = messages.value.find(m => m.message_id === messageId);
  if (msg) {
    console.log('[ChatArea] 找到消息，标记为已撤回');
    msg.recalled = true;
    // 强制 Vue 重新渲染
    messages.value = [...messages.value];
  } else {
    console.warn('[ChatArea] 未找到 message_id 为', messageId, '的消息，重新加载消息列表');
    // 重新加载消息列表（从数据库）
    await loadChatMessages();
  }
};

// 暴露方法供父组件调用
defineExpose({
  addMessage,
  loadChatMessages,
  handleMessageRecalled,
});

</script>

<template>
  <div class="chat-area">
    <!-- 聊天头部 -->
    <div class="chat-header">
      <div class="chat-title">
        <div class="chat-avatar">
          <img 
            v-if="chatId && chatType && !chatAvatarFailed" 
            :src="chatType === 'private' ? `asset://avatar/user/${chatId}.png` : `asset://avatar/group/${chatId}.png`" 
            :alt="chatName"
            class="avatar-image"
            @error="chatAvatarFailed = true"
          />
          <div v-else class="avatar-placeholder">
            {{ chatName ? chatName.charAt(0) : '?' }}
          </div>
        </div>
        <span class="chat-name">{{ chatName || '选择聊天' }}</span>
      </div>
      <!-- Debug 按钮（仅开发环境显示） -->
      <button 
        v-if="isDev" 
        class="debug-button" 
        @click="showDebugPanel = !showDebugPanel"
        :title="showDebugPanel ? '隐藏调试面板' : '显示调试面板'"
      >
        <svg width="16" height="16" viewBox="0 0 16 16" fill="none" xmlns="http://www.w3.org/2000/svg">
          <rect x="2" y="3" width="12" height="10" rx="1.5" stroke="currentColor" stroke-width="1.5" fill="none"/>
          <line x1="2" y1="6" x2="14" y2="6" stroke="currentColor" stroke-width="1.5"/>
          <line x1="4.5" y1="8.5" x2="6.5" y2="8.5" stroke="currentColor" stroke-width="1.2" stroke-linecap="round"/>
          <line x1="4.5" y1="10.5" x2="8.5" y2="10.5" stroke="currentColor" stroke-width="1.2" stroke-linecap="round"/>
        </svg>
        <span>Debug</span>
      </button>
    </div>
    
    <!-- Debug 面板（仅开发环境显示） -->
    <div v-if="isDev && showDebugPanel" class="debug-panel">
      <div class="debug-panel-header">
        <h3>调试面板</h3>
        <button class="debug-close-button" @click="showDebugPanel = false">×</button>
      </div>
      <div class="debug-panel-tabs">
        <button 
          class="debug-tab"
          :class="{ active: debugActiveTab === 'messages' }"
          @click="debugActiveTab = 'messages'"
        >
          消息列表
        </button>
        <button 
          v-if="chatType === 'group'"
          class="debug-tab"
          :class="{ active: debugActiveTab === 'members' }"
          @click="debugActiveTab = 'members'"
        >
          群成员 ({{ groupMembersList.length }})
        </button>
      </div>
      <div class="debug-panel-content">
        <!-- 消息列表 Tab -->
        <div v-if="debugActiveTab === 'messages'" class="debug-tab-content">
          <div class="debug-info">
            <p><strong>总消息数：</strong>{{ messages.length }}</p>
            <p><strong>当前聊天消息数：</strong>{{ filteredMessages.length }}</p>
          </div>
          <div class="debug-messages">
            <h4>所有消息实体：</h4>
            <div class="debug-message-list">
              <div 
                v-for="(msg, index) in messages" 
                :key="msg.localMessageId || index"
                class="debug-message-item"
              >
                <div class="debug-message-header">
                  <span class="debug-message-index">#{{ index + 1 }}</span>
                  <span class="debug-message-type">{{ msg.post_type }}</span>
                  <span v-if="msg.message_id" class="debug-message-id">ID: {{ msg.message_id }}</span>
                  <span v-if="msg.localMessageId" class="debug-local-id">Local: {{ msg.localMessageId.substring(0, 8) }}...</span>
                </div>
                <pre class="debug-message-json">{{ JSON.stringify(msg, null, 2) }}</pre>
              </div>
            </div>
          </div>
        </div>
        
        <!-- 群成员列表 Tab -->
        <div v-if="debugActiveTab === 'members' && chatType === 'group'" class="debug-tab-content">
          <div class="debug-info">
            <p><strong>群 ID：</strong>{{ chatId }}</p>
            <p><strong>群成员数：</strong>{{ groupMembersList.length }}</p>
          </div>
          <div class="debug-members">
            <h4>群成员列表：</h4>
            <div v-if="groupMembersList.length === 0" class="debug-empty">
              暂无群成员数据，请稍候...
            </div>
            <div v-else class="debug-member-list">
              <div 
                v-for="member in groupMembersList"
                :key="member.userId"
                class="debug-member-item"
              >
                <div class="debug-member-header">
                  <span class="debug-member-id">{{ member.userId }}</span>
                  <span class="debug-member-name">{{ member.card || member.nickname }}</span>
                  <span v-if="member.role" class="debug-member-role">{{ member.role }}</span>
                </div>
                <div class="debug-member-info">
                  <span v-if="member.card"><strong>群名片：</strong>{{ member.card }}</span>
                  <span><strong>昵称：</strong>{{ member.nickname }}</span>
                  <span v-if="member.title"><strong>头衔：</strong>{{ member.title }}</span>
                </div>
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>

    <!-- 消息列表 -->
    <div class="messages-container" ref="messagesContainer">
      <div v-if="groupedMessages.length === 0" class="empty-state">
        <p>暂无消息</p>
        <p class="hint">开始聊天吧！</p>
      </div>
      <template v-for="(group, groupIndex) in groupedMessages" :key="`group-${groupIndex}`">
        <!-- 时间显示 -->
        <div v-if="group.showTime" class="message-time">
          {{ formatTime(group.time) }}
        </div>
        
        <!-- 发送人组 -->
        <div 
          class="message-item"
          :class="[group.messages[0].message_type, { 'message-sent': group.messages[0].post_type === 'message_sent' }]"
        >
          <div class="message-content">
            <!-- 头像区域（群组且是他人消息时，在最后一条消息左侧显示） -->
            <div v-if="chatType === 'group' && group.showSender && group.userId" class="message-avatar">
              <img 
                :src="`asset://avatar/user/${group.userId}.png`" 
                :alt="group.senderName"
                class="message-avatar-image"
                @error="(e: Event) => { (e.target as HTMLImageElement).style.display = 'none'; }"
              />
            </div>
            
            <!-- 消息气泡组 -->
            <div class="message-bubbles">
              <!-- 发送人名称（群组且显示发送人时，在第一条消息上方显示） -->
              <div v-if="chatType === 'group' && group.showSender" class="message-sender">
                {{ group.senderName }}
              </div>
              
              <div
                v-for="(msg, msgIndex) in group.messages"
                :key="`${msg.time}-${msg.message_id || msgIndex}`"
                class="message-text"
                :class="{ 
                  'single-image': isSingleImage(parseMessage(msg)),
                  'image-only': isImageOnlyMessage(parseMessage(msg)) && !isSingleImage(parseMessage(msg)),
                  'message-bubble': true,
                  'message-recalled': msg.recalled
                }"
                @contextmenu="(e: MouseEvent) => showMessageContextMenu(e, msg)"
              >
                <div v-if="msg.recalled" class="recalled-notice">此消息已被撤回</div>
                
                <!-- 被回复的消息 -->
                <div v-if="!msg.recalled && getReplyMessage(parseMessage(msg))" class="reply-quote">
                  <div class="reply-quote-line"></div>
                  <div class="reply-quote-content">
                    <div class="reply-quote-sender">
                      {{ getSenderName(getReplyMessage(parseMessage(msg))!) }}
                    </div>
                    <div class="reply-quote-text">
                      <template v-for="item in renderReplyMessage(getReplyMessage(parseMessage(msg))!)" :key="item.key">
                        <span>{{ item.content }}</span>
                      </template>
                    </div>
                  </div>
                </div>
                
                <!-- 消息内容 -->
                <template v-for="item in renderMessage(parseMessage(msg))" :key="item.key">
                  <span v-if="item.type === 'text'">{{ item.content }}</span>
                  <template v-else-if="item.type === 'image'">
                    <!-- base64 图片（data URI）直接显示 -->
                    <img
                      v-if="item.url.startsWith('data:')"
                      :src="item.url"
                      alt="图片"
                      class="message-image"
                      @click.stop="openImageViewer(item.url)"
                      @error="() => { imageFailed.set(item.url, true); }"
                    />
                    <!-- URL 图片（需要下载和缓存） -->
                    <template v-else>
                      <img
                        v-if="imageCache.get(item.url) && !imageFailed.get(item.url)"
                        :data-image-url="item.url"
                        :data-image-file="item.file || ''"
                        :src="`asset://localhost/${imageCache.get(item.url)}`"
                        alt="图片"
                        class="message-image"
                        @click.stop="openImageViewer(`asset://localhost/${imageCache.get(item.url)}`)"
                        @error="() => { imageFailed.set(item.url, true); }"
                      />
                      <div v-else-if="!imageFailed.get(item.url)" :data-image-url="item.url" :data-image-file="item.file || ''" ref="imagePlaceholderRef" class="image-placeholder">
                        <span class="image-loading">图片加载中...</span>
                      </div>
                      <div v-else class="image-error">
                        <span>[图片加载失败]</span>
                      </div>
                    </template>
                  </template>
                  <template v-else-if="item.type === 'face'">
                    <img 
                      v-if="getFaceImageUrl(item.id)" 
                      :src="getFaceImageUrl(item.id)!" 
                      :alt="getFaceDisplayText(item.id)"
                      class="cq-face-image"
                      @error="(e: Event) => { 
                        // 图片加载失败时，隐藏图片
                        const img = e.target as HTMLImageElement;
                        if (img) {
                          img.style.display = 'none';
                        }
                      }"
                    />
                    <span v-else class="cq-face">{{ getFaceDisplayText(item.id) }}</span>
                  </template>
                  <span v-else-if="item.type === 'at'" class="cq-at">{{ item.displayName || `@${item.qq}` }}</span>
                  <div v-else-if="item.type === 'forward'" class="forward-message" @click="openForwardMessage(item.id)">
                    <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="currentColor" class="forward-icon">
                      <path d="M20 8h-3V4H3c-1.1 0-2 .9-2 2v11h2c0 1.66 1.34 3 3 3s3-1.34 3-3h6c0 1.66 1.34 3 3 3s3-1.34 3-3h2v-5l-3-4zM6 18.5c-.83 0-1.5-.67-1.5-1.5s.67-1.5 1.5-1.5 1.5.67 1.5 1.5-.67 1.5-1.5 1.5zm13.5-9l1.96 2.5H17V9.5h2.5zm-1.5 9c-.83 0-1.5-.67-1.5-1.5s.67-1.5 1.5-1.5 1.5.67 1.5 1.5-.67 1.5-1.5 1.5z"/>
                    </svg>
                    <span>查看聊天记录</span>
                  </div>
                </template>
              </div>
            </div>
          </div>
        </div>
      </template>
    </div>

    <!-- 输入区域 -->
    <div class="input-area" v-if="chatId && chatType">
      <!-- 选中的图片预览 -->
      <div v-if="selectedImages.length > 0" class="selected-images">
        <div
          v-for="(image, index) in selectedImages"
          :key="index"
          class="selected-image-item"
        >
          <img :src="image.preview" alt="预览" class="preview-image" />
          <button
            @click="removeSelectedImage(index)"
            class="remove-image-button"
            title="移除"
          >
            ×
          </button>
        </div>
      </div>
      
      <!-- 回复消息预览 -->
      <div v-if="replyToMessage" class="reply-preview">
        <div class="reply-preview-content">
          <div class="reply-preview-header">
            <span class="reply-preview-label">回复 {{ getSenderName(replyToMessage) }}</span>
            <button class="reply-preview-close" @click="cancelReply" title="取消回复">×</button>
          </div>
          <div class="reply-preview-text">
            {{ getMessagePreviewText(parseMessage(replyToMessage)) }}
          </div>
        </div>
      </div>
      
      <div class="input-container">
        <input
          ref="fileInputRef"
          type="file"
          accept="image/*"
          multiple
          style="display: none"
          @change="handleImageSelect"
        />
        <!-- 图片按钮（左侧） -->
        <button
          @click="fileInputRef?.click()"
          class="image-button"
          :disabled="sending"
          title="附加图片"
        >
          <svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="currentColor">
            <path d="M16.5 6v11.5c0 2.21-1.79 4-4 4s-4-1.79-4-4V5a2.5 2.5 0 0 1 5 0v10.5c0 .55-.45 1-1 1s-1-.45-1-1V6H10v9.5a2.5 2.5 0 0 0 5 0V5c0-2.21-1.79-4-4-4S7 2.79 7 5v12.5c0 3.04 2.46 5.5 5.5 5.5s5.5-2.46 5.5-5.5V6h-1.5z"/>
          </svg>
        </button>
        
        <!-- 表情选择器气泡 -->
        <div v-if="showFacePicker" ref="facePickerRef" class="face-picker-popup">
          <div class="face-picker-grid">
            <div
              v-for="face in faceList"
              :key="face.id"
              class="face-item"
              :title="face.name"
              @click="selectFace(face.id)"
            >
              <img
                v-if="face.url"
                :src="face.url"
                :alt="face.name"
                class="face-image"
              />
              <span v-else class="face-name">{{ face.name }}</span>
            </div>
          </div>
        </div>
        
        <!-- @ 选择器气泡 -->
        <div 
          v-if="showMentionPicker && chatType === 'group'" 
          ref="mentionPickerRef" 
          class="mention-picker-popup"
          :style="{ 
            bottom: mentionPickerPosition.bottom !== undefined ? mentionPickerPosition.bottom + 'px' : undefined,
            top: mentionPickerPosition.top !== undefined ? mentionPickerPosition.top + 'px' : undefined,
            left: mentionPickerPosition.left + 'px' 
          }"
        >
          <div v-if="filteredMemberList.length === 0" class="mention-picker-empty">
            没有找到成员
          </div>
          <div v-else class="mention-picker-list">
            <div
              v-for="(member, index) in filteredMemberList"
              :key="member.userId"
              class="mention-item"
              :class="{ selected: index === selectedMentionIndex }"
              @click="selectMention(member.userId, member.card || member.nickname)"
            >
              <img 
                :src="`asset://avatar/user/${member.userId}.png`" 
                :alt="member.card || member.nickname"
                class="mention-avatar"
                @error="(e: Event) => { (e.target as HTMLImageElement).style.display = 'none'; }"
              />
              <div class="mention-info">
                <div class="mention-name">{{ member.card || member.nickname }}</div>
                <div class="mention-id">{{ member.userId }}</div>
              </div>
            </div>
          </div>
        </div>
        
        <div
          ref="inputEditorRef"
          :contenteditable="!sending"
          class="rich-input-editor"
          :class="{ disabled: sending }"
          data-placeholder="输入消息... (Enter 发送, Shift+Enter 换行, Cmd/Ctrl+V 粘贴图片)"
          @input="handleEditorInput"
          @paste="handleEditorPaste"
          @keydown="handleEditorKeyDown"
          @compositionstart="isComposing = true; compositionEndTime = 0"
          @compositionupdate="isComposing = true"
          @compositionend="isComposing = false; compositionEndTime = Date.now()"
        ></div>
        
        <!-- 表情按钮（右侧） -->
        <button
          @click.stop="toggleFacePicker"
          class="face-button"
          :disabled="sending"
          title="选择表情"
          :class="{ active: showFacePicker }"
        >
          <svg xmlns="http://www.w3.org/2000/svg" width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
            <rect x="3" y="3" width="7" height="7" rx="2"></rect>
            <rect x="14" y="3" width="7" height="7" rx="2"></rect>
            <rect x="14" y="14" width="7" height="7" rx="2"></rect>
            <rect x="3" y="14" width="7" height="7" rx="2"></rect>
          </svg>
        </button>
      </div>
    </div>
    <div v-else class="input-placeholder">
      选择一个聊天开始对话
    </div>

    <!-- 右键菜单 -->
    <div
      v-if="showContextMenu"
      class="context-menu"
      :style="{ left: `${contextMenuX}px`, top: `${contextMenuY}px` }"
      @click.stop
    >
      <div class="context-menu-item" @click="replyMessage">
        <span>回复</span>
      </div>
      <div 
        v-if="canRecallMessage(contextMenuMessage)"
        class="context-menu-item" 
        @click="recallMessage"
      >
        <span>撤回消息</span>
      </div>
    </div>

    <!-- 点击菜单外部关闭 -->
    <div
      v-if="showContextMenu"
      class="context-menu-overlay"
      @click="closeContextMenu"
    ></div>
  </div>
</template>

<style scoped>
.chat-area {
  display: flex;
  flex-direction: column;
  height: 100%;
  background: #f4f4f5;
}

.chat-header {
  padding: 12px 20px;
  background: white;
  border-bottom: 1px solid #e8e8e8;
  display: flex;
  justify-content: space-between;
  align-items: center;
}

.chat-title {
  display: flex;
  align-items: center;
  gap: 12px;
  flex: 1;
}

.debug-button {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 6px 12px;
  background: #0088cc;
  color: white;
  border: none;
  border-radius: 8px;
  cursor: pointer;
  font-size: 12px;
  font-weight: 500;
  transition: background 0.15s;
}

.debug-button svg {
  flex-shrink: 0;
}

.debug-button span {
  line-height: 1;
}

.debug-button:hover {
  background: #006699;
}

.debug-panel {
  position: fixed;
  top: 0;
  right: 0;
  width: 500px;
  height: 100vh;
  background: white;
  box-shadow: -2px 0 8px rgba(0, 0, 0, 0.15);
  z-index: 1000;
  display: flex;
  flex-direction: column;
  overflow: hidden;
}

.debug-panel-header {
  padding: 12px 16px;
  background: #0088cc;
  color: white;
  display: flex;
  justify-content: space-between;
  align-items: center;
  border-bottom: 1px solid #e8e8e8;
}

.debug-panel-header h3 {
  margin: 0;
  font-size: 16px;
  font-weight: 600;
}

.debug-close-button {
  background: transparent;
  border: none;
  color: white;
  font-size: 24px;
  cursor: pointer;
  padding: 0;
  width: 24px;
  height: 24px;
  display: flex;
  align-items: center;
  justify-content: center;
  line-height: 1;
}

.debug-close-button:hover {
  opacity: 0.8;
}

.debug-panel-content {
  flex: 1;
  overflow-y: auto;
  padding: 16px;
}

/* Debug Panel Tabs */
.debug-panel-tabs {
  display: flex;
  border-bottom: 1px solid #e5e5e5;
  background-color: #fafafa;
}

.debug-tab {
  flex: 0 0 auto;
  padding: 12px 20px;
  background: none;
  border: none;
  font-size: 14px;
  color: #666;
  cursor: pointer;
  position: relative;
  transition: color 0.2s;
}

.debug-tab:hover {
  color: #333;
}

.debug-tab.active {
  color: #0088cc;
  font-weight: 600;
}

.debug-tab.active::after {
  content: '';
  position: absolute;
  bottom: 0;
  left: 0;
  right: 0;
  height: 2px;
  background-color: #0088cc;
}

.debug-tab-content {
  display: flex;
  flex-direction: column;
  gap: 16px;
}

.debug-info {
  margin-bottom: 16px;
  padding: 12px;
  background: #f4f4f5;
  border-radius: 8px;
}

.debug-info p {
  margin: 4px 0;
  font-size: 14px;
  color: #222;
}

.debug-messages h4 {
  margin: 0 0 12px 0;
  font-size: 14px;
  font-weight: 600;
  color: #222;
}

.debug-messages,
.debug-members {
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.debug-members h4 {
  margin: 0 0 12px 0;
  font-size: 14px;
  font-weight: 600;
  color: #222;
}

.debug-empty {
  text-align: center;
  color: #999;
  padding: 40px 20px;
  font-size: 14px;
}

.debug-member-list {
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.debug-member-item {
  border: 1px solid #e8e8e8;
  border-radius: 8px;
  overflow: hidden;
  background: #fafafa;
  padding: 12px;
}

.debug-member-header {
  display: flex;
  gap: 8px;
  margin-bottom: 8px;
  flex-wrap: wrap;
  align-items: center;
}

.debug-member-id {
  padding: 2px 8px;
  background: #e3f2fd;
  color: #1976d2;
  border-radius: 4px;
  font-size: 12px;
  font-weight: 500;
  font-family: monospace;
}

.debug-member-name {
  padding: 2px 8px;
  background: #f3e5f5;
  color: #7b1fa2;
  border-radius: 4px;
  font-size: 12px;
  font-weight: 600;
}

.debug-member-role {
  padding: 2px 8px;
  background: #fff3e0;
  color: #f57c00;
  border-radius: 4px;
  font-size: 12px;
  font-weight: 500;
}

.debug-member-info {
  display: flex;
  flex-direction: column;
  gap: 4px;
  font-size: 12px;
  color: #666;
}

.debug-member-info span {
  line-height: 1.5;
}

.debug-message-list {
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.debug-message-item {
  border: 1px solid #e8e8e8;
  border-radius: 8px;
  overflow: hidden;
  background: #fafafa;
}

.debug-message-header {
  padding: 8px 12px;
  background: #e7f2ff;
  border-bottom: 1px solid #e8e8e8;
  display: flex;
  gap: 8px;
  align-items: center;
  flex-wrap: wrap;
  font-size: 12px;
}

.debug-message-index {
  font-weight: 600;
  color: #0088cc;
}

.debug-message-type {
  padding: 2px 6px;
  background: #0088cc;
  color: white;
  border-radius: 4px;
  font-size: 11px;
}

.debug-message-id {
  color: #666;
}

.debug-local-id {
  color: #999;
  font-family: monospace;
}

.debug-message-json {
  padding: 12px;
  margin: 0;
  font-size: 11px;
  font-family: 'Courier New', monospace;
  background: white;
  overflow-x: auto;
  max-height: 300px;
  overflow-y: auto;
  white-space: pre-wrap;
  word-break: break-all;
}

.chat-avatar {
  flex-shrink: 0;
}

.chat-avatar .avatar-image {
  width: 42px;
  height: 42px;
  border-radius: 50%;
  object-fit: cover;
  background: #f4f4f5;
}

.chat-avatar .avatar-placeholder {
  width: 42px;
  height: 42px;
  border-radius: 50%;
  background: linear-gradient(135deg, #0088cc 0%, #006699 100%);
  color: white;
  display: flex;
  align-items: center;
  justify-content: center;
  font-size: 18px;
  font-weight: 600;
  box-shadow: 0 1px 3px rgba(0, 0, 0, 0.1);
}

.chat-name {
  font-size: 16px;
  font-weight: 600;
  color: #222;
}

.messages-container {
  flex: 1;
  overflow-y: auto;
  padding: 20px;
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.empty-state {
  flex: 1;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  color: #8e8e93;
}

.empty-state p {
  margin: 4px 0;
}

.hint {
  font-size: 13px;
}

.message-item {
  display: flex;
  flex-direction: column;
  margin-bottom: 8px;
}

.message-time {
  text-align: center;
  font-size: 12px;
  color: #8e8e93;
  margin: 12px 0;
  font-weight: 500;
}

.message-content {
  display: flex;
  flex-direction: row;
  align-items: flex-end;
  gap: 8px;
  max-width: 80%;
}

/* 群组消息：接收的消息靠左 */
.message-item.group .message-content {
  margin-left: 0;
  margin-right: auto;
}

/* 私聊消息：接收的消息靠左 */
.message-item.private:not(.message-sent) .message-content {
  margin-left: 0;
  margin-right: auto;
}

/* 发送的消息（包括私聊和群组）：靠右 */
.message-item.message-sent .message-content {
  margin-left: auto;
  margin-right: 0;
}

/* 头像区域（在消息左侧） */
.message-avatar {
  flex-shrink: 0;
  width: 40px;
  height: 40px;
  display: flex;
  align-items: flex-end;
  justify-content: center;
}

.message-avatar-image {
  width: 40px;
  height: 40px;
  border-radius: 50%;
  object-fit: cover;
  background: #f0f0f0;
}

/* 消息气泡组容器 */
.message-bubbles {
  flex: 1;
  display: flex;
  flex-direction: column;
  gap: 4px;
  min-width: 0;
}

/* 发送人名称（在第一条消息上方） */
.message-sender {
  font-size: 13px;
  color: #8e8e93;
  margin-bottom: 4px;
  font-weight: 500;
  padding: 0 2px;
}

.message-text {
  display: inline-block;
  max-width: 100%;
  background: white;
  padding: 10px 14px;
  border-radius: 12px;
  font-size: 15px;
  line-height: 1.4;
  word-wrap: break-word;
  box-shadow: 0 1px 2px rgba(0, 0, 0, 0.05);
  position: relative;
}

/* 回复引用样式 */
.reply-quote {
  display: flex;
  gap: 8px;
  margin-bottom: 8px;
  padding: 8px 10px;
  background: rgba(0, 0, 0, 0.03);
  border-radius: 8px;
  font-size: 13px;
}

.reply-quote-line {
  width: 3px;
  background: linear-gradient(to bottom, #0088cc, #006699);
  border-radius: 2px;
  flex-shrink: 0;
}

.reply-quote-content {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.reply-quote-sender {
  font-weight: 600;
  color: #0088cc;
  font-size: 12px;
}

.reply-quote-text {
  color: #666;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  line-height: 1.3;
}

/* 发送的消息中的回复引用 */
.message-sent .reply-quote {
  background: rgba(255, 255, 255, 0.2);
}

.message-sent .reply-quote-line {
  background: rgba(255, 255, 255, 0.5);
}

.message-sent .reply-quote-sender {
  color: rgba(255, 255, 255, 0.9);
}

.message-sent .reply-quote-text {
  color: rgba(255, 255, 255, 0.7);
}

/* 发送的消息（无论私聊还是群组）：蓝色背景 */
.message-item.message-sent .message-text {
  background: #0088cc;
  color: white;
  box-shadow: 0 1px 2px rgba(0, 0, 0, 0.08);
}

/* 接收的消息（私聊和群组）：白色背景 */
.message-item.private:not(.message-sent) .message-text,
.message-item.group:not(.message-sent) .message-text {
  background: white;
  color: #222;
}

/* 单个图片：透明背景，圆角 */
.message-text.single-image {
  background: transparent !important;
  padding: 0;
  border-radius: 8px;
  overflow: hidden;
}

.message-text.single-image img {
  border-radius: 8px;
  display: block;
}

/* 单个表情：没有气泡，不需要背景，不需要圆角 */
.message-text.single-face {
  background: transparent !important;
  padding: 0;
  border-radius: 0;
  box-shadow: none;
}

.message-text.single-face .cq-face-image {
  border-radius: 0;
  box-shadow: none;
}

/* 撤回的消息样式 */
.message-text.message-recalled {
  background: #f4f4f5 !important;
  color: #8e8e93 !important;
  opacity: 0.7;
  font-style: italic;
}

.message-item.message-sent .message-text.message-recalled {
  background: #e7f2ff !important;
  color: #8e8e93 !important;
}

.recalled-notice {
  font-size: 13px;
  color: #8e8e93;
  margin-bottom: 4px;
}

/* 右键菜单样式 */
.context-menu {
  position: fixed;
  background: white;
  border: 1px solid #e8e8e8;
  border-radius: 12px;
  box-shadow: 0 4px 16px rgba(0, 0, 0, 0.12);
  z-index: 1000;
  min-width: 120px;
  overflow: hidden;
}

.context-menu-item {
  padding: 12px 16px;
  cursor: pointer;
  font-size: 14px;
  color: #222;
  transition: background 0.15s;
}

.context-menu-item:hover {
  background: #f4f4f5;
}

.context-menu-overlay {
  position: fixed;
  top: 0;
  left: 0;
  right: 0;
  bottom: 0;
  z-index: 999;
  background: transparent;
}

/* 多个图片或表情：排列到气泡里（使用默认的气泡样式） */

.message-item.message-sent .message-content {
  margin-left: auto;
  margin-right: 0;
}

.message-image {
  max-width: 300px;
  max-height: 400px;
  border-radius: 8px;
  margin: 4px 0;
  display: block;
  cursor: pointer;
  object-fit: contain;
  transition: opacity 0.2s;
}

.message-image:hover {
  opacity: 0.9;
}

.image-placeholder {
  display: inline-block;
  padding: 20px 40px;
  background: rgba(0, 0, 0, 0.04);
  border-radius: 12px;
  margin: 4px 0;
  color: #8e8e93;
  font-size: 13px;
}

.image-loading {
  display: inline-block;
}

.image-error {
  display: inline-block;
  padding: 20px 40px;
  background: rgba(255, 59, 48, 0.1);
  border: 1px solid rgba(255, 59, 48, 0.3);
  border-radius: 12px;
  margin: 4px 0;
  color: #ff3b30;
  font-size: 13px;
}

.image-error span {
  display: inline-block;
}

.cq-face,
.cq-at {
  color: #0088cc;
  font-weight: 500;
}

.cq-face-image {
  width: 26px;
  height: 26px;
  vertical-align: middle;
  display: inline-block;
  object-fit: contain;
}

.cq-at {
  background: rgba(0, 136, 204, 0.1);
  padding: 3px 8px;
  border-radius: 6px;
}

/* 自己发送的消息中的 @ 标签样式（白色背景，蓝色文字） */
.message-item.message-sent .cq-at {
  color: white;
  background: rgba(255, 255, 255, 0.2);
  font-weight: 600;
}

/* 合并转发消息样式 */
.forward-message {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  padding: 8px 12px;
  background: #f0f9ff;
  border: 1px solid #0088cc;
  border-radius: 8px;
  cursor: pointer;
  color: #0088cc;
  font-size: 14px;
  transition: all 0.15s;
  user-select: none;
}

.forward-message:hover {
  background: #e0f2ff;
  border-color: #0077b3;
}

.forward-icon {
  flex-shrink: 0;
}

.message-item.message-sent .forward-message {
  background: rgba(255, 255, 255, 0.15);
  border-color: rgba(255, 255, 255, 0.4);
  color: white;
}

.message-item.message-sent .forward-message:hover {
  background: rgba(255, 255, 255, 0.25);
  border-color: rgba(255, 255, 255, 0.6);
}

.input-area {
  padding: 16px;
  background: white;
  border-top: 1px solid #e8e8e8;
}

/* 回复消息预览 */
.reply-preview {
  margin-bottom: 12px;
  padding: 10px 12px;
  background: #f0f9ff;
  border-left: 3px solid #0088cc;
  border-radius: 6px;
}

.reply-preview-content {
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.reply-preview-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
}

.reply-preview-label {
  font-size: 13px;
  color: #0088cc;
  font-weight: 500;
}

.reply-preview-close {
  background: none;
  border: none;
  color: #8e8e93;
  font-size: 20px;
  cursor: pointer;
  padding: 0;
  width: 20px;
  height: 20px;
  display: flex;
  align-items: center;
  justify-content: center;
  border-radius: 4px;
  transition: background 0.15s, color 0.15s;
}

.reply-preview-close:hover {
  background: rgba(0, 0, 0, 0.05);
  color: #333;
}

.reply-preview-text {
  font-size: 13px;
  color: #666;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  max-width: 100%;
}

/* 富文本编辑器样式 */
.rich-input-editor {
  flex: 1;
  padding: 12px 14px;
  border: 1px solid #e8e8e8;
  border-radius: 12px;
  font-size: 15px;
  font-family: inherit;
  line-height: 1.4;
  min-height: 48px;
  max-height: 120px;
  overflow-y: auto;
  word-wrap: break-word;
  white-space: pre-wrap;
  background: white;
  outline: none;
  transition: border-color 0.15s, box-shadow 0.15s;
}

.rich-input-editor:focus {
  border-color: #0088cc;
  box-shadow: 0 0 0 3px rgba(0, 136, 204, 0.1);
}

.rich-input-editor:empty:before {
  content: attr(data-placeholder);
  color: #8e8e93;
  pointer-events: none;
}

.rich-input-editor.disabled {
  background: #f4f4f5;
  cursor: not-allowed;
  opacity: 0.6;
  pointer-events: none;
}

/* 输入框中的表情样式 */
.input-face-emoji {
  vertical-align: middle;
  display: inline-block;
  width: 22px;
  height: 22px;
  margin: 0 2px;
  object-fit: contain;
}

/* 输入框中的 @ 用户样式 */
.input-mention {
  color: #0088cc !important;
  background-color: rgba(0, 136, 204, 0.1) !important;
  padding: 2px 6px !important;
  border-radius: 4px !important;
  display: inline-block !important;
  margin: 0 2px !important;
  cursor: pointer !important;
  font-weight: 500 !important;
  vertical-align: baseline !important;
}

.input-mention:hover {
  background-color: rgba(0, 136, 204, 0.15) !important;
}

.selected-images {
  display: flex;
  gap: 8px;
  padding: 8px 16px;
  overflow-x: auto;
  border-bottom: 1px solid #e8e8e8;
}

.selected-image-item {
  position: relative;
  flex-shrink: 0;
}

.preview-image {
  width: 80px;
  height: 80px;
  object-fit: cover;
  border-radius: 12px;
  border: 1px solid #e8e8e8;
}

.remove-image-button {
  position: absolute;
  top: -8px;
  right: -8px;
  width: 24px;
  height: 24px;
  border-radius: 50%;
  background: #ff3b30;
  color: white;
  border: none;
  cursor: pointer;
  font-size: 18px;
  line-height: 1;
  display: flex;
  align-items: center;
  justify-content: center;
  box-shadow: 0 2px 6px rgba(0, 0, 0, 0.2);
  transition: background 0.15s;
}

.remove-image-button:hover {
  background: #d32f2f;
}

.input-container {
  display: flex;
  gap: 12px;
  align-items: flex-end;
  position: relative;
}

.face-button,
.image-button {
  padding: 12px;
  background-color: #f4f4f5;
  color: #8e8e93;
  border: 1px solid #e8e8e8;
  border-radius: 12px;
  cursor: pointer;
  transition: all 0.15s;
  display: flex;
  align-items: center;
  justify-content: center;
  flex-shrink: 0;
}

.face-button svg,
.image-button svg {
  width: 20px;
  height: 20px;
}

.face-button:hover:not(:disabled),
.image-button:hover:not(:disabled) {
  background-color: #e8e8e8;
  color: #222;
}

.face-button.active {
  background-color: #0088cc;
  color: white;
  border-color: #0088cc;
}

.face-button:disabled,
.image-button:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

/* 表情选择器气泡 */
.face-picker-popup {
  position: absolute;
  bottom: 100%;
  right: 0;
  margin-bottom: 8px;
  background: white;
  border: 1px solid #e8e8e8;
  border-radius: 16px;
  box-shadow: 0 4px 16px rgba(0, 0, 0, 0.12);
  padding: 16px;
  width: 450px;
  max-height: min(500px, calc(100vh - 200px));
  overflow-y: auto;
  overflow-x: auto;
  z-index: 1000;
}

.face-picker-grid {
  display: grid;
  grid-template-columns: repeat(10, 1fr);
  gap: 8px;
}

.face-item {
  width: 32px;
  height: 32px;
  display: flex;
  align-items: center;
  justify-content: center;
  cursor: pointer;
  border-radius: 8px;
  transition: background-color 0.15s;
  overflow: hidden;
}

.face-item:hover {
  background-color: #f4f4f5;
}

.face-image {
  width: 100%;
  height: 100%;
  object-fit: contain;
}

.face-name {
  font-size: 11px;
  text-align: center;
  color: #8e8e93;
  padding: 2px;
}

/* @ 选择器样式 */
.mention-picker-popup {
  position: absolute;
  background: white;
  border: 1px solid #e5e5e5;
  border-radius: 12px;
  box-shadow: 0 4px 12px rgba(0, 0, 0, 0.15);
  padding: 8px;
  width: 280px;
  max-height: 300px;
  overflow-y: auto;
  z-index: 1001;
}

.mention-picker-empty {
  padding: 20px;
  text-align: center;
  color: #8e8e93;
  font-size: 14px;
}

.mention-picker-list {
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.mention-item {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 8px 10px;
  border-radius: 8px;
  cursor: pointer;
  transition: background-color 0.15s;
}

.mention-item:hover {
  background-color: #f4f4f5;
}

.mention-item.selected {
  background-color: #e8f4ff;
  border: 1px solid #0088cc;
}

.mention-avatar {
  width: 36px;
  height: 36px;
  border-radius: 50%;
  object-fit: cover;
  background: #f0f0f0;
  flex-shrink: 0;
}

.mention-info {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.mention-name {
  font-size: 14px;
  font-weight: 500;
  color: #222;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.mention-id {
  font-size: 12px;
  color: #8e8e93;
  font-family: monospace;
}



.input-placeholder {
  padding: 40px;
  text-align: center;
  color: #8e8e93;
  font-size: 15px;
  background: white;
  border-top: 1px solid #e8e8e8;
}
</style>


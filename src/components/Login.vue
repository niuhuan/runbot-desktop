<script setup lang="ts">
import { ref, onMounted, onBeforeUnmount } from 'vue';
import { listen } from '@tauri-apps/api/event';
import { runbotService } from '../services/runbot';
import { useConnectionState, initConnectionStore } from '../stores/connection';
import { loadConfig, saveConfig, updateConfig } from '../services/config';

const emit = defineEmits<{
  connected: []
}>();

const wsUrl = ref('ws://127.0.0.1:8080');
const accessToken = ref('');
// 使用全局连接状态
const { status: connectionStatus } = useConnectionState();
const isConnecting = ref(false);
const errorMessage = ref('');
const hasSelfId = ref(false); // 是否已获取到 self_id

let statusUnlisten: (() => void) | null = null;
let selfIdUnlisten: (() => void) | null = null;
let selfIdTimeout: ReturnType<typeof setTimeout> | null = null; // self_id 获取超时定时器
const SELF_ID_TIMEOUT = 10000; // 10秒超时

// 加载保存的配置
onMounted(async () => {
  // 初始化全局连接状态管理
  await initConnectionStore();

  // 等待一小段时间，确保如果是从 MainView 切换过来的，配置已经更新完成
  await new Promise(resolve => setTimeout(resolve, 200));

  const savedConfig = await loadConfig();
  let shouldAutoConnect = false;
  if (savedConfig) {
    wsUrl.value = savedConfig.wsUrl;
    accessToken.value = savedConfig.accessToken || '';
    
    // 如果上次是登录状态，且有 wsUrl，则自动连接
    // 注意：初始化时连接状态通常是 'disconnected'，这是正常的，不应该阻止自动登录
    // 但是，如果连接状态是 'disconnected' 且刚刚切换过来，可能是用户主动退出登录
    // 所以需要再次检查配置，确保 lastConnected 确实为 true
    if (savedConfig.lastConnected && savedConfig.wsUrl) {
      // 再次读取配置，确保获取到最新的值（防止时序问题）
      const latestConfig = await loadConfig();
      if (latestConfig && latestConfig.lastConnected && latestConfig.wsUrl) {
        shouldAutoConnect = true;
        console.log('[Login] 检测到上次登录状态，准备自动连接');
      } else {
        console.log('[Login] 配置已更新，取消自动连接');
      }
    }
  }
  
  // 设置状态监听（全局状态会自动更新，这里只需要处理业务逻辑）
  statusUnlisten = await runbotService.onStatusChange(async (status) => {
    console.log('[Login] 连接状态变化:', status);
    
    if (status.status === 'connected') {
      console.log('[Login] 连接成功，检查是否已有 self_id:', hasSelfId.value);
      
      // 清除之前的超时定时器
      if (selfIdTimeout) {
        clearTimeout(selfIdTimeout);
        selfIdTimeout = null;
      }
      
      // 连接成功，检查是否已经有 self_id
      if (hasSelfId.value) {
        console.log('[Login] 已有 self_id，触发 connected 事件');
        emit('connected');
      } else {
        console.log('[Login] 等待 self_id 事件...');
        // 启动超时定时器：如果10秒内没有获取到 self_id，断开连接
        selfIdTimeout = setTimeout(async () => {
          console.log('[Login] 获取 self_id 超时，断开连接');
          errorMessage.value = '获取用户信息超时，请检查连接配置';
          isConnecting.value = false;
          hasSelfId.value = false;
          
          // 断开连接
          try {
            await runbotService.disconnect();
          } catch (error) {
            console.error('断开连接失败:', error);
          }
          
          // 获取 self_id 超时，清除自动登录标志
          await updateConfig({ lastConnected: false });
          
          selfIdTimeout = null;
        }, SELF_ID_TIMEOUT);
      }
    } else if (status.status === 'error') {
      // 清除超时定时器
      if (selfIdTimeout) {
        clearTimeout(selfIdTimeout);
        selfIdTimeout = null;
      }
      errorMessage.value = status.message || '连接失败';
      isConnecting.value = false;
      hasSelfId.value = false; // 重置
      // 连接失败，清除自动登录标志
      await updateConfig({ lastConnected: false });
    } else if (status.status === 'connecting') {
      // 清除之前的超时定时器
      if (selfIdTimeout) {
        clearTimeout(selfIdTimeout);
        selfIdTimeout = null;
      }
      isConnecting.value = true;
      errorMessage.value = '';
      hasSelfId.value = false; // 重置
    } else if (status.status === 'disconnected') {
      // 清除超时定时器
      if (selfIdTimeout) {
        clearTimeout(selfIdTimeout);
        selfIdTimeout = null;
      }
      hasSelfId.value = false; // 重置
      // 断开连接时，清除自动登录标志（防止自动重连）
      await updateConfig({ lastConnected: false });
    }
  });

  // 监听 self_id 事件
  selfIdUnlisten = await listen<number>('runbot-self-id', async (event) => {
    console.log('[Login] 收到 runbot-self-id 事件:', event.payload);
    const selfId = event.payload;
    if (selfId && selfId > 0) {
      console.log('[Login] self_id 有效，更新状态');
      
      // 清除超时定时器
      if (selfIdTimeout) {
        clearTimeout(selfIdTimeout);
        selfIdTimeout = null;
      }
      
      hasSelfId.value = true;
      // 保存配置时使用 self_id
      await saveConfig({
        wsUrl: wsUrl.value,
        accessToken: accessToken.value || undefined,
        lastConnected: true,
      }, selfId);
      
      // 只有在连接成功且获取到 self_id 后才进入主页面
      console.log('[Login] 检查连接状态:', connectionStatus.status);
      if (connectionStatus.status === 'connected') {
        console.log('[Login] 连接成功且获取到 self_id，触发 connected 事件');
        emit('connected');
      } else {
        console.log('[Login] 连接状态不是 connected，等待连接成功');
        // 如果连接状态不是 connected，等待连接成功
        // 连接成功后会自动检查 hasSelfId 并触发 connected 事件
      }
    } else {
      console.log('[Login] self_id 无效:', selfId);
      // self_id 无效，断开连接
      errorMessage.value = '获取到的用户信息无效';
      isConnecting.value = false;
      try {
        await runbotService.disconnect();
      } catch (error) {
        console.error('断开连接失败:', error);
      }
      // self_id 无效，清除自动登录标志
      await updateConfig({ lastConnected: false });
    }
  });
  
  // 所有监听器设置完成后，如果需要自动连接，则执行
  if (shouldAutoConnect) {
    console.log('[Login] 检测到上次登录状态，自动连接...');
    // 延迟一下，确保 UI 已经渲染完成，监听器已经设置好
    setTimeout(() => {
      handleConnect();
    }, 500);
  }
});

onBeforeUnmount(() => {
  // 清除超时定时器
  if (selfIdTimeout) {
    clearTimeout(selfIdTimeout);
    selfIdTimeout = null;
  }
  
  if (statusUnlisten) {
    statusUnlisten();
  }
  if (selfIdUnlisten) {
    selfIdUnlisten();
  }
});

// 连接服务器
const handleConnect = async () => {
  if (!wsUrl.value.trim()) {
    errorMessage.value = '请输入 WebSocket URL';
    return;
  }

  // 清除之前的超时定时器
  if (selfIdTimeout) {
    clearTimeout(selfIdTimeout);
    selfIdTimeout = null;
  }

  isConnecting.value = true;
  errorMessage.value = '';
  hasSelfId.value = false; // 重置

  try {
    // 保存配置（暂时不使用 self_id，等连接成功后从消息中获取）
    await saveConfig({
      wsUrl: wsUrl.value,
      accessToken: accessToken.value || undefined,
      lastConnected: true,
    });

    // 连接服务器
    await runbotService.connect(wsUrl.value, accessToken.value || undefined);
    // 注意：isConnecting 会在状态变化时更新，这里不需要手动设置
  } catch (error: any) {
    errorMessage.value = error.message || '连接失败';
    isConnecting.value = false;
    hasSelfId.value = false;
    // 连接失败，清除自动登录标志
    await updateConfig({ lastConnected: false });
    console.error('连接失败:', error);
  }
};

// 回车键连接
const handleKeyPress = (event: KeyboardEvent) => {
  if (event.key === 'Enter' && !isConnecting.value) {
    handleConnect();
  }
};

onBeforeUnmount(() => {
  if (statusUnlisten) {
    statusUnlisten();
  }
});
</script>

<template>
  <div class="login-container">
    <div class="login-card">
      <div class="login-header">
        <h1>Runbot Desktop</h1>
        <p class="subtitle">OneBot v11 客户端</p>
      </div>

      <div class="login-form">
        <div class="form-group">
          <label for="ws-url">WebSocket URL</label>
          <input
            id="ws-url"
            v-model="wsUrl"
            type="text"
            placeholder="ws://127.0.0.1:8080"
            :disabled="isConnecting"
            @keypress="handleKeyPress"
          />
          <small class="form-hint">输入 Runbot 服务器的 WebSocket 地址</small>
        </div>

        <div class="form-group">
          <label for="access-token">Access Token (可选)</label>
          <input
            id="access-token"
            v-model="accessToken"
            type="password"
            placeholder="留空表示不使用 token"
            :disabled="isConnecting"
            @keypress="handleKeyPress"
          />
          <small class="form-hint">如果需要认证，请输入 Access Token</small>
        </div>

        <div v-if="errorMessage" class="error-message">
          {{ errorMessage }}
        </div>

        <div v-if="connectionStatus.status === 'connecting'" class="status-message connecting">
          <span class="status-dot"></span>
          正在连接...
        </div>
        <div v-if="connectionStatus.status === 'connected' && !hasSelfId" class="status-message connecting">
          <span class="status-dot"></span>
          连接成功，正在获取用户信息...
        </div>

        <button
          class="connect-button"
          :disabled="isConnecting || !wsUrl.trim()"
          @click="handleConnect"
        >
          {{ isConnecting ? '连接中...' : '连接' }}
        </button>
      </div>

      <div class="login-footer">
        <p class="help-text">
          首次使用？请确保 Runbot 服务器正在运行，并配置正确的 WebSocket 地址。
        </p>
      </div>
    </div>
  </div>
</template>

<style scoped>
.login-container {
  width: 100vw;
  height: 100vh;
  display: flex;
  align-items: center;
  justify-content: center;
  background: linear-gradient(135deg, #667eea 0%, #764ba2 100%);
  padding: 20px;
}

.login-card {
  background: white;
  border-radius: 16px;
  box-shadow: 0 20px 60px rgba(0, 0, 0, 0.3);
  padding: 40px;
  width: 100%;
  max-width: 450px;
  animation: slideUp 0.3s ease-out;
}

@keyframes slideUp {
  from {
    opacity: 0;
    transform: translateY(20px);
  }
  to {
    opacity: 1;
    transform: translateY(0);
  }
}

.login-header {
  text-align: center;
  margin-bottom: 32px;
}

.login-header h1 {
  font-size: 32px;
  font-weight: 700;
  color: #333;
  margin: 0 0 8px 0;
}

.subtitle {
  color: #666;
  font-size: 14px;
  margin: 0;
}

.login-form {
  display: flex;
  flex-direction: column;
  gap: 20px;
}

.form-group {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.form-group label {
  font-size: 14px;
  font-weight: 600;
  color: #333;
}

.form-group input {
  padding: 12px 16px;
  border: 2px solid #e0e0e0;
  border-radius: 8px;
  font-size: 14px;
  transition: all 0.2s;
  font-family: inherit;
}

.form-group input:focus {
  outline: none;
  border-color: #667eea;
  box-shadow: 0 0 0 3px rgba(102, 126, 234, 0.1);
}

.form-group input:disabled {
  background-color: #f5f5f5;
  cursor: not-allowed;
}

.form-hint {
  font-size: 12px;
  color: #999;
  margin-top: 4px;
}

.error-message {
  padding: 12px;
  background-color: #ffebee;
  color: #c62828;
  border-radius: 8px;
  font-size: 14px;
  border: 1px solid #ffcdd2;
}

.status-message {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 12px;
  border-radius: 8px;
  font-size: 14px;
}

.status-message.connecting {
  background-color: #fff3e0;
  color: #e65100;
}

.status-dot {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  background-color: #ff9800;
  animation: pulse 1.5s ease-in-out infinite;
}

@keyframes pulse {
  0%, 100% {
    opacity: 1;
  }
  50% {
    opacity: 0.5;
  }
}

.connect-button {
  padding: 14px 24px;
  background: linear-gradient(135deg, #667eea 0%, #764ba2 100%);
  color: white;
  border: none;
  border-radius: 8px;
  font-size: 16px;
  font-weight: 600;
  cursor: pointer;
  transition: all 0.2s;
  margin-top: 8px;
}

.connect-button:hover:not(:disabled) {
  transform: translateY(-2px);
  box-shadow: 0 8px 20px rgba(102, 126, 234, 0.4);
}

.connect-button:active:not(:disabled) {
  transform: translateY(0);
}

.connect-button:disabled {
  opacity: 0.6;
  cursor: not-allowed;
}

.login-footer {
  margin-top: 24px;
  text-align: center;
}

.help-text {
  font-size: 12px;
  color: #999;
  margin: 0;
  line-height: 1.5;
}
</style>


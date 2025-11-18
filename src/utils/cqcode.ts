/**
 * CQ 码解析工具
 * 解析 OneBot 消息中的 CQ 码
 */

export interface CQSegment {
  type: 'text' | 'image' | 'face' | 'at' | 'unknown';
  data: Record<string, string>;
  text?: string; // 对于 text 类型，存储文本内容
}

/**
 * 解析 CQ 码
 * @param message 消息内容
 * @returns CQ 段数组
 */
export function parseCQCode(message: string): CQSegment[] {
  if (!message) return [];
  
  const segments: CQSegment[] = [];
  let currentIndex = 0;
  
  while (currentIndex < message.length) {
    // 查找下一个 [CQ:
    const cqStart = message.indexOf('[CQ:', currentIndex);
    
    if (cqStart === -1) {
      // 没有更多 CQ 码，添加剩余文本
      const remainingText = message.substring(currentIndex);
      if (remainingText) {
        segments.push({
          type: 'text',
          data: {},
          text: remainingText,
        });
      }
      break;
    }
    
    // 添加 CQ 码之前的文本
    if (cqStart > currentIndex) {
      const textBefore = message.substring(currentIndex, cqStart);
      segments.push({
        type: 'text',
        data: {},
        text: textBefore,
      });
    }
    
    // 查找 CQ 码的结束位置 ]
    const cqEnd = message.indexOf(']', cqStart);
    if (cqEnd === -1) {
      // 没有找到结束符，添加剩余文本
      const remainingText = message.substring(cqStart);
      segments.push({
        type: 'text',
        data: {},
        text: remainingText,
      });
      break;
    }
    
    // 解析 CQ 码内容
    const cqContent = message.substring(cqStart + 4, cqEnd); // 跳过 "[CQ:"
    const parts = cqContent.split(',');
    const type = parts[0]?.trim() || 'unknown';
    
    // 解析参数
    const data: Record<string, string> = {};
    for (let i = 1; i < parts.length; i++) {
      const part = parts[i].trim();
      const equalIndex = part.indexOf('=');
      if (equalIndex !== -1) {
        const key = part.substring(0, equalIndex).trim();
        let value = part.substring(equalIndex + 1).trim();
        // 解码 HTML 实体（如 &amp; -> &）
        value = value.replace(/&amp;/g, '&').replace(/&lt;/g, '<').replace(/&gt;/g, '>').replace(/&quot;/g, '"').replace(/&#39;/g, "'");
        data[key] = value;
      } else {
        // 如果没有等号，可能是只有 key 没有 value 的参数
        if (part) {
          data[part] = '';
        }
      }
    }
    
    segments.push({
      type: type as CQSegment['type'],
      data,
    });
    
    currentIndex = cqEnd + 1;
  }
  
  return segments;
}

/**
 * 提取消息中的所有图片 URL
 * @param message 消息内容
 * @returns 图片 URL 数组
 */
export function extractImageUrls(message: string): string[] {
  const segments = parseCQCode(message);
  const urls: string[] = [];
  
  for (const segment of segments) {
    if (segment.type === 'image') {
      // 优先使用 url 参数，如果没有则使用 file 参数
      const url = segment.data.url || segment.data.file;
      if (url) {
        urls.push(url);
      }
    }
  }
  
  return urls;
}


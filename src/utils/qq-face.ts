/**
 * QQ 表情工具
 * 使用 qface 包获取表情信息，并通过 asset 协议访问打包的表情文件
 */
import qface from 'qface';

/**
 * 根据表情 ID 获取表情名称
 * @param faceId 表情 ID
 * @returns 表情名称，如果未找到则返回默认值
 */
export function getFaceName(faceId: number | string): string {
  const id = String(faceId);
  const face = qface.get(id);
  if (face && face.QDes) {
    // QDes 格式是 "/惊讶"，去掉前面的斜杠
    return face.QDes.substring(1) || `表情:${id}`;
  }
  return `表情:${id}`;
}

/**
 * 根据表情 ID 获取表情显示文本
 * @param faceId 表情 ID
 * @returns 表情显示文本
 */
export function getFaceDisplayText(faceId: number | string): string {
  const name = getFaceName(faceId);
  // 如果找到了名称，显示名称，否则显示 ID
  if (name.startsWith('表情:')) {
    return `[${name}]`;
  }
  return `[${name}]`;
}

/**
 * 根据表情 ID 获取表情图片 URL（使用 asset 协议）
 * @param faceId 表情 ID
 * @returns 表情图片 URL，如果未找到或只支持 gif 则返回 null（静态表情不打包）
 */
export function getFaceImageUrl(faceId: number | string): string | null {
  const id = String(faceId);
  const face = qface.get(id);
  if (!face) {
    return null;
  }
  
  // 只返回 gif 表情的 URL，静态表情（png）不打包，返回 null 让前端显示文本
  if (face.isStatic === '1') {
    // 静态表情不打包，返回 null 使用文本显示
    return null;
  } else {
    // 动态表情（gif）已打包，返回 URL
    return `asset://qface/gif/s${id}.gif`;
  }
}


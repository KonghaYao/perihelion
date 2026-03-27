// utils/hooks.js — 显式 Signal 订阅 hooks（兼容 esm.sh 版本差异）
import { useState, useEffect } from 'https://esm.sh/preact/hooks'

/**
 * 显式订阅一个 Signal，当值变化时触发组件重渲染。
 * 不依赖 @preact/signals 对 Preact options 的自动 patch，
 * 兼容 esm.sh 多版本共存场景。
 */
export function useSignalValue(signal) {
  const [value, setValue] = useState(() => signal.value)
  useEffect(() => {
    // 立即同步一次，防止 useEffect 延迟导致错过更新
    setValue(signal.value)
    return signal.subscribe(v => setValue(v))
  }, [signal])
  return value
}

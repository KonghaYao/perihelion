// app.js — Preact 渲染入口
import { h, render } from 'https://esm.sh/preact'
import htm from 'https://esm.sh/htm'
import { markedReady } from './state.js'
import { App } from './components/App.js'

const html = htm.bind(h)

// ─── 动态加载 UMD CDN 脚本 ────────────────────────────────────

function loadScript(src) {
  return new Promise((res, rej) => {
    const s = document.createElement('script')
    s.src = src
    s.onload = res
    s.onerror = rej
    document.head.appendChild(s)
  })
}

// 并行加载 UMD 脚本（不阻塞 Preact 初始化）
// marked/hljs/DOMPurify 不提供 ES Module 格式，通过 script 标签加载为全局变量
Promise.allSettled([
  loadScript('https://cdnjs.cloudflare.com/ajax/libs/highlight.js/11.9.0/highlight.min.js'),
  loadScript('https://cdn.jsdelivr.net/npm/marked@15/marked.min.js'),
  loadScript('https://cdnjs.cloudflare.com/ajax/libs/dompurify/3.0.6/purify.min.js'),
]).then(() => {
  // CDN 脚本加载完毕，MessageList 组件读取此 signal 后升级为 Markdown 渲染
  markedReady.value = true
})

// 立即 mount，不等待 CDN
// 消息先以纯文本展示，CDN 就绪后 markedReady signal 触发自动升级渲染
render(html`<${App} />`, document.getElementById('app'))

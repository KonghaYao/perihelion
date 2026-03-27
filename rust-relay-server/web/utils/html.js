// utils/html.js — 统一导出 htm 绑定后的 html 标签函数
// 所有组件文件直接 import { html } from '../utils/html.js'，避免重复 htm.bind(h)
import { h } from 'https://esm.sh/preact'
import htm from 'https://esm.sh/htm'

export const html = htm.bind(h)

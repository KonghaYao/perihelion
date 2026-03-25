---
name: web-crawler
description: 使用 @langgraph-js/web-fetch CLI 抓取网页内容并转换为 Markdown，同时支持通过搜索网站（bing.com、google.com 等）进行搜索。当用户需要爬取网页、搜索信息、提取文章正文、批量抓取 URL、将 HTML 转为 Markdown，或读取飞书/Docker Hub/InfoQ 等平台文档时使用此 skill。
---

# web-crawler

通过 `npx` 直接调用 CLI，无需安装。

## 搜索功能

访问搜索网站即可替代独立搜索工具。直接在 URL 中拼接查询参数即可：

| 搜索网站            | URL 格式                                  |
| ------------------- | ----------------------------------------- |
| Bing                | `https://www.bing.com/search?q=<query>`   |
| 百度 不要使用       | `https://www.baidu.com/s?wd=<query>`      |
| Google 不要使用     | `https://www.google.com/search?q=<query>` |
| DuckDuckGo 不要使用 | `https://duckduckgo.com/?q=<query>`       |

**示例：搜索 Rust 异步编程：**

```bash
npx @langgraph-js/web-fetch "https://www.bing.com/search?q=Rust+async+programming+tutorial" | head -c 5000
```

**搜索结果提取建议：**

- Bing/Google 搜索结果通常在前 5000 字符内包含前 5-10 条结果
- 使用 `--extract-depth advanced` 可更完整提取结构化内容
- 搜索结果页面可能包含广告和追踪脚本，内容截断后通常足够用于判断相关性

**进阶：带参数搜索**

```bash
# Bing 限定中文结果
npx @langgraph-js/web-fetch "https://www.bing.com/search?q=Rust+async+programming&hl=zh-CN" | head -c 5000

# Bing 限定最近一年
npx @langgraph-js/web-fetch "https://www.bing.com/search?q=Rust+async&tbs=qdr:y" | head -c 5000
```

## 基本用法

```bash
npx @langgraph-js/web-fetch https://example.com
```

## 选项

| 选项               | 默认值   | 说明                            |
| ------------------ | -------- | ------------------------------- |
| `--format`         | markdown | 输出格式：`markdown` \| `text`  |
| `--extract-depth`  | basic    | 提取深度：`basic` \| `advanced` |
| `--include-images` | false    | 是否提取图片链接                |
| `--timeout`        | 30       | 超时秒数（1–60）                |

## 长度控制（重要）

单页内容可能超过 10 万字符，直接放入上下文会导致 token 爆炸。**必须**用 `head` 截断输出：

```bash
npx @langgraph-js/web-fetch https://example.com | head -c 8000
```

**参考上限**

| 场景         | 建议截断上限  |
| ------------ | ------------- |
| 单 URL 精读  | 8 000 字节    |
| 2–5 URL 并发 | 3 000 字节/页 |
| 6+ URL 批量  | 1 500 字节/页 |

多 URL 时单独调用并各自截断，避免合并后溢出：

```bash
for url in https://a.com https://b.com; do
    npx @langgraph-js/web-fetch "$url" | head -c 3000
    echo "---"
done
```

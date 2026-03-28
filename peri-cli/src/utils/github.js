import { CONFIG } from './config.js';
import fetch from 'node-fetch';

// 获取 GitHub 发布列表
export async function getReleases(options = { perPage: 5 }) {
  const url = `${CONFIG.apiUrl}/repos/${CONFIG.owner}/${CONFIG.repo}/releases?per_page=${options.perPage}`;

  const response = await fetch(url, {
    headers: {
      'Accept': 'application/vnd.github.v3+json',
      'User-Agent': 'perihelion-cli'
    }
  });

  if (!response.ok) {
    throw new Error(`Failed to fetch releases: ${response.statusText}`);
  }

  return response.json();
}

// 获取最新版本
export async function getLatestRelease() {
  const url = `${CONFIG.apiUrl}/repos/${CONFIG.owner}/${CONFIG.repo}/releases/latest`;

  const response = await fetch(url, {
    headers: {
      'Accept': 'application/vnd.github.v3+json',
      'User-Agent': 'perihelion-cli'
    }
  });

  if (!response.ok) {
    throw new Error(`Failed to fetch latest release: ${response.statusText}`);
  }

  return response.json();
}

// 获取指定版本
export async function getReleaseByVersion(tag) {
  const url = `${CONFIG.apiUrl}/repos/${CONFIG.owner}/${CONFIG.repo}/releases/tags/${tag}`;

  const response = await fetch(url, {
    headers: {
      'Accept': 'application/vnd.github.v3+json',
      'User-Agent': 'perihelion-cli'
    }
  });

  if (!response.ok) {
    throw new Error(`Failed to fetch release ${tag}: ${response.statusText}`);
  }

  return response.json();
}

// 查找匹配平台的二进制文件
export function findAssetForPlatform(release, platformInfo) {
  const target = platformInfo.target;

  // 使用 config.js 中生成的 platformStr（格式：aarch64-macos / x86_64-linux 等）
  // 但资源命名是 macos-aarch64 格式，需要转换
  const parts = platformInfo.platformStr.split('-');
  const assetPlatformStr = `${parts[1]}-${parts[0]}`; // aarch64-macos -> macos-aarch64

  return release.assets.find(asset => {
    const name = asset.name.toLowerCase();

    // 查找 agent-tui 的二进制文件（资源命名：agent-tui-macos-aarch64）
    if (name.includes('agent-tui') && name.includes(assetPlatformStr)) {
      return true;
    }

    // 兼容旧的命名方式（如果存在）
    if (name.includes('rust-agent-tui') && name.includes(target)) {
      return true;
    }

    return false;
  });
}

// 格式化发布信息
export function formatReleaseInfo(release) {
  const publishedAt = new Date(release.published_at).toLocaleDateString();
  return {
    tag: release.tag_name,
    name: release.name || release.tag_name,
    publishedAt,
    url: release.html_url,
    assets: release.assets.map(a => ({
      name: a.name,
      size: formatBytes(a.size),
      downloadUrl: a.browser_download_url
    }))
  };
}

// 格式化字节数
function formatBytes(bytes) {
  if (bytes === 0) return '0 Bytes';

  const k = 1024;
  const sizes = ['Bytes', 'KB', 'MB', 'GB'];
  const i = Math.floor(Math.log(bytes) / Math.log(k));

  return Math.round(bytes / Math.pow(k, i) * 100) / 100 + ' ' + sizes[i];
}

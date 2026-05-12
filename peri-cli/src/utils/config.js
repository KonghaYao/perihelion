import fs from "fs-extra";
import path from "path";

// 从 tag 中提取工具名：agent-v1.17 → agent
export function extractToolName(tag) {
    const match = tag.match(/^(.+?)-v-?\d/);
    return match ? match[1] : tag;
}

// GitHub 下载代理（环境变量 GITHUB_PROXY 或 PERI_GITHUB_PROXY）
// 代理 URL 替换原始 URL 中的 "https://github.com" 前缀
// 例如：GITHUB_PROXY=https://<your-proxy-url>  /https://github.com peri install agent
// 原始地址：https://github.com/KonghaYao/peri/releases/download/...
// 代理地址：https://<your-proxy-url>  /https://github.com/KonghaYao/peri/releases/download/...
export function getDownloadUrl(originalUrl) {
    const proxy = process.env.PERI_GITHUB_PROXY || process.env.GITHUB_PROXY;
    if (proxy) {
        return originalUrl.replace("https://github.com", proxy);
    }
    return originalUrl;
}

// GitHub API 配置
export const CONFIG = {
    owner: "konghayao",
    repo: "perihelion",
    apiUrl: "https://api.github.com",
};

// 平台信息
export function getPlatformInfo() {
    const platform = process.platform;
    const arch = process.arch;

    let rustPlatform;
    let rustArch;

    // 转换为 Rust 的目标三元组
    switch (platform) {
        case "darwin":
            rustPlatform = "apple-darwin";
            break;
        case "linux":
            rustPlatform = "unknown-linux-gnu";
            break;
        case "win32":
            rustPlatform = "pc-windows-msvc";
            break;
        default:
            throw new Error(`Unsupported platform: ${platform}`);
    }

    switch (arch) {
        case "x64":
            rustArch = "x86_64";
            break;
        case "arm64":
            rustArch = "aarch64";
            break;
        default:
            throw new Error(`Unsupported architecture: ${arch}`);
    }

    return {
        platform,
        arch,
        target: `${rustArch}-${rustPlatform}`,
        platformStr: `${rustArch === "x86_64" ? "x86_64" : rustArch}-${platform === "darwin" ? "macos" : platform === "linux" ? "linux" : "windows"}`,
        isWindows: platform === "win32",
        isMac: platform === "darwin",
        isLinux: platform === "linux",
    };
}

// 生成安装目录路径
export function getInstallDir() {
    const homeDir = process.env.HOME || process.env.USERPROFILE;
    return `${homeDir}/.perihelion`;
}

// 生成可执行文件路径
export function getExecutablePath(version = "current") {
    const installDir = getInstallDir();
    const platformInfo = getPlatformInfo();

    const toolName = extractToolName(version);
    const binName = platformInfo.isWindows ? `${toolName}.exe` : toolName;
    return `${installDir}/${version}/${binName}`;
}

// 获取当前安装版本
export async function getCurrentVersion() {
    const installDir = getInstallDir();
    const versionFile = `${installDir}/current-version.txt`;

    try {
        if (await fs.pathExists(versionFile)) {
            return (await fs.readFile(versionFile, "utf-8")).trim();
        }
    } catch (error) {
        // 文件不存在或读取失败
    }

    return null;
}

// 创建符号链接：每个包用自己的 link，agent 额外创建 peri 别名
async function createSymlink(execPath, linkPath, isWindows) {
    if (isWindows) {
        await fs.copy(execPath, linkPath);
    } else {
        try {
            await fs.unlink(linkPath);
        } catch {
            // 文件不存在，忽略
        }
        await fs.symlink(execPath, linkPath);
    }
}

// 查找版本目录中的实际二进制文件（兼容旧版 agent-tui 命名）
async function findBinary(version, platformInfo) {
    const installDir = getInstallDir();
    const toolName = extractToolName(version);
    const ext = platformInfo.isWindows ? ".exe" : "";

    // 优先新命名，fallback 旧命名（agent → agent-tui）
    const candidates = [`${installDir}/${version}/${toolName}${ext}`];
    if (toolName === "agent") {
        candidates.push(`${installDir}/${version}/agent-tui${ext}`);
    }

    for (const p of candidates) {
        if (await fs.pathExists(p)) return p;
    }
    return candidates[0];
}

// 设置当前版本
export async function setCurrentVersion(version) {
    const installDir = getInstallDir();
    const platformInfo = getPlatformInfo();

    // 确保安装目录存在
    await fs.ensureDir(installDir);

    const execPath = await findBinary(version, platformInfo);
    const toolName = extractToolName(version);
    const ext = platformInfo.isWindows ? ".exe" : "";

    // agent 包创建 peri 别名，其他包创建包名 symlink
    if (toolName === "agent") {
        await createSymlink(
            execPath,
            `${installDir}/peri${ext}`,
            platformInfo.isWindows,
        );
        await fs.writeFile(`${installDir}/current-version.txt`, version);
    } else {
        await createSymlink(
            execPath,
            `${installDir}/${toolName}${ext}`,
            platformInfo.isWindows,
        );
    }
}

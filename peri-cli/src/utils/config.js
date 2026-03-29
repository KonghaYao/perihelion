import fs from 'fs-extra';
import path from 'path';

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

    const binName = platformInfo.isWindows ? "agent-tui.exe" : "agent-tui";
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

// 设置当前版本
export async function setCurrentVersion(version) {
    const installDir = getInstallDir();

    // 确保安装目录存在
    await fs.ensureDir(installDir);

    // 写入版本文件
    await fs.writeFile(`${installDir}/current-version.txt`, version);

    // 创建符号链接（Unix）或复制文件（Windows）
    const execPath = getExecutablePath(version);
    const linkPath = `${installDir}/peri${getPlatformInfo().isWindows ? ".exe" : ""}`;

    if (getPlatformInfo().isWindows) {
        await fs.copy(execPath, linkPath);
    } else {
        try {
            await fs.unlink(linkPath);
        } catch (error) {
            // 文件不存在，忽略
        }
        await fs.symlink(execPath, linkPath);
    }
}

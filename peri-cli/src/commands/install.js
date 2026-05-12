import fs from "fs-extra";
import path from "path";
import fetch from "node-fetch";
import {
    getPlatformInfo,
    getInstallDir,
    getExecutablePath,
    setCurrentVersion,
    getDownloadUrl,
} from "../utils/config.js";
import {
    getLatestRelease,
    getLatestReleaseByPrefix,
    getReleaseByVersion,
    findAssetForPlatform,
    extractToolName,
} from "../utils/github.js";
import { clean } from "./clean.js";

/**
 * 判断输入是完整 tag 还是包名前缀
 * 完整 tag 格式：xxx-vN.N（含版本号）
 * 包名前缀：agent（不含版本号）
 */
function isFullTag(input) {
    return /-v\d/.test(input);
}

export async function install(pkg) {
    const platformInfo = getPlatformInfo();
    const installDir = getInstallDir();

    console.log(`🔍 Detecting platform: ${platformInfo.target}`);

    try {
        let release;

        if (!pkg) {
            // 无参数 → 安装最新 agent
            console.log("📦 Fetching latest agent release...");
            release = await getLatestRelease();
        } else if (isFullTag(pkg)) {
            // 完整 tag（如 agent-v1.17）→ 安装指定版本
            console.log(`📦 Installing version: ${pkg}`);
            release = await getReleaseByVersion(pkg);
        } else {
            // 包名前缀（如 agent）→ 安装该包的最新版本
            console.log(`📦 Fetching latest ${pkg} release...`);
            release = await getLatestReleaseByPrefix(`${pkg}-`);
        }

        console.log(`✅ Found version: ${release.tag_name}`);

        // 查找匹配平台的二进制文件
        const asset = findAssetForPlatform(release, platformInfo);
        if (!asset) {
            console.error(
                `❌ No binary found for platform: ${platformInfo.target}`,
            );
            console.log("Available assets:");
            release.assets.forEach((a) => console.log(`  - ${a.name}`));
            process.exit(1);
        }

        console.log(`📥 Found binary: ${asset.name}`);

        // 创建安装目录
        const versionDir = path.join(installDir, release.tag_name);
        await fs.ensureDir(versionDir);

        // 下载二进制文件
        const toolName = extractToolName(release.tag_name);
        const targetPath = path.join(
            versionDir,
            platformInfo.isWindows ? `${toolName}.exe` : toolName,
        );
        const downloadUrl = getDownloadUrl(asset.browser_download_url);
        console.log(`⬇️  Downloading to: ${targetPath}`);
        if (downloadUrl !== asset.browser_download_url) {
            console.log(`   Via proxy: ${downloadUrl}`);
        }

        const response = await fetch(downloadUrl);
        if (!response.ok) {
            throw new Error(`Download failed: ${response.statusText}`);
        }

        const fileStream = fs.createWriteStream(targetPath);
        await new Promise((resolve, reject) => {
            response.body.pipe(fileStream);
            response.body.on("error", reject);
            fileStream.on("finish", resolve);
            fileStream.on("error", reject);
        });

        // 设置可执行权限
        if (!platformInfo.isWindows) {
            await fs.chmod(targetPath, "755");
        }

        console.log("✅ Download complete!");

        // 设置当前版本
        await setCurrentVersion(release.tag_name);

        console.log("✅ Download complete!");

        // 自动清理旧版本
        console.log("");
        await clean();
        console.log("");
        console.log(`Version: ${release.tag_name}`);
        console.log(`Binary: ${targetPath}`);
        console.log("");
        console.log("To add peri to your PATH, run:");
        console.log("  npx peri-cli add-env");
        console.log("");
    } catch (error) {
        console.error("❌ Installation failed:", error.message);
        if (error.stack) {
            console.error(error.stack);
        }
        process.exit(1);
    }
}

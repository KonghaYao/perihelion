import fs from "fs-extra";
import path from "path";
import fetch from "node-fetch";
import {
    getPlatformInfo,
    getInstallDir,
    getExecutablePath,
    setCurrentVersion,
} from "../utils/config.js";
import {
    getLatestRelease,
    getReleaseByVersion,
    findAssetForPlatform,
} from "../utils/github.js";

export async function install(options) {
    const platformInfo = getPlatformInfo();
    const installDir = getInstallDir();

    console.log(`🔍 Detecting platform: ${platformInfo.target}`);

    try {
        // 获取要安装的版本
        let release;
        if (options.version) {
            console.log(`📦 Installing version: ${options.version}`);
            release = await getReleaseByVersion(options.version);
        } else {
            console.log("📦 Fetching latest release...");
            release = await getLatestRelease();
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
        const targetPath = path.join(
            versionDir,
            platformInfo.isWindows ? "agent-tui.exe" : "agent-tui",
        );
        console.log(`⬇️  Downloading to: ${targetPath}`);

        const response = await fetch(asset.browser_download_url);
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

        console.log("");
        console.log("🎉 Installation complete!");
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

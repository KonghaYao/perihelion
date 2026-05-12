import fs from "fs-extra";
import { getInstallDir, extractToolName } from "../utils/config.js";

// 从 tag 中提取版本号用于排序：agent-v1.17 → [1,17]
function parseVersion(tag) {
    const match = tag.match(/-v-?([\d.]+)/);
    return match ? match[1].split(".").map(Number) : [0];
}

/**
 * 清理旧版本，每个包只保留最新的 N 个版本
 */
export async function clean(keepCount = 2) {
    const installDir = getInstallDir();

    if (!(await fs.pathExists(installDir))) {
        console.log("Nothing to clean.");
        return;
    }

    const entries = await fs.readdir(installDir);

    // 按工具名分组，只处理版本目录（匹配 xxx-v 格式）
    const groups = {};
    for (const entry of entries) {
        const match = entry.match(/^(.+)-v-?\d/);
        if (!match) continue;
        const toolName = match[1];
        if (!groups[toolName]) groups[toolName] = [];
        groups[toolName].push(entry);
    }

    let removed = 0;

    for (const [toolName, versions] of Object.entries(groups)) {
        // 按版本号排序（旧 → 新）
        versions.sort((a, b) => {
            const va = parseVersion(a);
            const vb = parseVersion(b);
            for (let i = 0; i < Math.max(va.length, vb.length); i++) {
                const na = va[i] || 0;
                const nb = vb[i] || 0;
                if (na !== nb) return na - nb;
            }
            return 0;
        });

        // 保留最新的 N 个
        const toRemove = versions.slice(0, Math.max(0, versions.length - keepCount));

        for (const version of toRemove) {
            const dir = `${installDir}/${version}`;
            // 跳过正在使用的版本（symlink 指向的目录）
            let isActive = false;
            try {
                const linkTarget = await fs.realpath(`${installDir}/${toolName}`);
                const dirReal = await fs.realpath(dir);
                if (linkTarget.startsWith(dirReal)) isActive = true;
            } catch {
                // symlink 不存在，忽略
            }
            try {
                const periTarget = await fs.realpath(`${installDir}/peri`);
                const dirReal = await fs.realpath(dir);
                if (periTarget.startsWith(dirReal)) isActive = true;
            } catch {
                // 忽略
            }

            if (isActive) continue;

            await fs.remove(dir);
            console.log(`  Removed: ${version}`);
            removed++;
        }
    }

    if (removed === 0) {
        console.log("✅ Already clean, nothing to remove.");
    } else {
        console.log(`\n🧹 Cleaned ${removed} old version(s).`);
    }
}

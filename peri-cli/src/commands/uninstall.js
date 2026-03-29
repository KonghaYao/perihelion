import fs from 'fs-extra';
import path from 'path';
import os from 'os';
import { getInstallDir, getPlatformInfo } from '../utils/config.js';

const SHELL_CONFIGS = {
  bash: { configFile: '.bashrc', marker: '# >>> peri >>>', markerEnd: '# <<< peri <<<' },
  zsh: { configFile: '.zshrc', marker: '# >>> peri >>>', markerEnd: '# <<< peri <<<' },
  fish: { configFile: '.config/fish/config.fish', marker: '# >>> peri >>>', markerEnd: '# <<< peri <<<' },
};

function detectShell() {
  const shell = process.env.SHELL || '/bin/bash';
  if (shell.includes('zsh')) return 'zsh';
  if (shell.includes('fish')) return 'fish';
  return 'bash';
}

export async function uninstall() {
  const installDir = getInstallDir();
  const platformInfo = getPlatformInfo();

  // 检查是否已安装
  if (!await fs.pathExists(installDir)) {
    console.log('⚠️  peri is not installed.');
    return;
  }

  console.log('🗑️  Uninstalling peri...');

  // 删除安装目录
  await fs.remove(installDir);
  console.log(`   Removed: ${installDir}`);

  // 清理 shell 配置
  if (!platformInfo.isWindows) {
    const shell = detectShell();
    const shellConfig = SHELL_CONFIGS[shell];
    const homeDir = os.homedir();
    const configPath = path.join(homeDir, shellConfig.configFile);

    if (await fs.pathExists(configPath)) {
      const content = await fs.readFile(configPath, 'utf-8');

      if (content.includes(shellConfig.marker)) {
        // 移除标记之间的内容
        const lines = content.split('\n');
        const newLines = [];
        let inBlock = false;

        for (const line of lines) {
          if (line.includes(shellConfig.marker)) {
            inBlock = true;
            continue;
          }
          if (line.includes(shellConfig.markerEnd)) {
            inBlock = false;
            continue;
          }
          if (!inBlock) {
            newLines.push(line);
          }
        }

        await fs.writeFile(configPath, newLines.join('\n'));
        console.log(`   Cleaned: ~/${shellConfig.configFile}`);
      }
    }
  }

  console.log('');
  console.log('✅ peri has been uninstalled.');
  console.log('');
  console.log('To apply changes in current session, run:');
  console.log(`   source ~/.${detectShell()}rc`);
}

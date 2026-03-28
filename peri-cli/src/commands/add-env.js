import fs from 'fs-extra';
import path from 'path';
import os from 'os';
import { getInstallDir, getPlatformInfo } from '../utils/config.js';

const SHELL_CONFIGS = {
  bash: {
    configFile: '.bashrc',
    marker: '# >>> perihelion >>>',
    markerEnd: '# <<< perihelion <<<',
  },
  zsh: {
    configFile: '.zshrc',
    marker: '# >>> perihelion >>>',
    markerEnd: '# <<< perihelion <<<',
  },
  fish: {
    configFile: '.config/fish/config.fish',
    marker: '# >>> perihelion >>>',
    markerEnd: '# <<< perihelion <<<',
  },
};

function detectShell() {
  const shell = process.env.SHELL || '/bin/bash';

  if (shell.includes('zsh')) return 'zsh';
  if (shell.includes('fish')) return 'fish';
  return 'bash';
}

function getEnvContent(installDir, shell) {
  if (shell === 'fish') {
    return `set -gx PATH ${installDir} \$PATH`;
  }

  return `export PATH="${installDir}:$PATH"`;
}

export async function addEnv() {
  const installDir = getInstallDir();
  const platformInfo = getPlatformInfo();
  const binPath = path.join(installDir, 'perihelion');

  // 检查二进制文件是否存在
  if (!await fs.pathExists(binPath)) {
    console.error('❌ Perihelion binary not found.');
    console.log('Please run `perihelion install` first.');
    process.exit(1);
  }

  // Windows 不支持 shell 配置文件注入
  if (platformInfo.isWindows) {
    console.log('');
    console.log('📝 On Windows, please manually add to your PATH:');
    console.log(`   ${installDir}`);
    console.log('');
    console.log('Or run this in PowerShell (current session only):');
    console.log(`   $env:Path = "${installDir};" + $env:Path`);
    return;
  }

  const shell = detectShell();
  const shellConfig = SHELL_CONFIGS[shell];
  const homeDir = os.homedir();
  const configPath = path.join(homeDir, shellConfig.configFile);

  // 确保配置文件存在
  await fs.ensureDir(path.dirname(configPath));
  if (!await fs.pathExists(configPath)) {
    await fs.writeFile(configPath, '');
  }

  const content = await fs.readFile(configPath, 'utf-8');

  // 检查是否已经添加过
  if (content.includes(shellConfig.marker)) {
    console.log('✅ Perihelion is already in your PATH.');
    console.log('');
    console.log('To activate in current session, run:');
    console.log(`   source ${configPath}`);
    return;
  }

  // 添加环境变量
  const envContent = `\n${shellConfig.marker}\n${getEnvContent(installDir, shell)}\n${shellConfig.markerEnd}\n`;

  await fs.appendFile(configPath, envContent);

  console.log('✅ Added Perihelion to your PATH.');
  console.log('');
  console.log(`Shell: ${shell}`);
  console.log(`Config: ~/${shellConfig.configFile}`);
  console.log('');
  console.log('To activate in current session, run:');
  console.log(`   source ${configPath}`);
  console.log('');
  console.log('Or start a new terminal session.');
}

import { getLatestRelease } from '../utils/github.js';
import { getCurrentVersion } from '../utils/config.js';
import { install } from './install.js';

export async function update() {
  try {
    const currentVersion = await getCurrentVersion();

    if (!currentVersion) {
      console.log('⚠️  Perihelion is not installed yet.');
      console.log('Run `perihelion install` to install the latest version.');
      return;
    }

    console.log(`🔍 Current version: ${currentVersion}`);
    console.log('🔍 Checking for updates...');

    const latestRelease = await getLatestRelease();

    if (latestRelease.tag_name === currentVersion) {
      console.log('✅ You are already on the latest version!');
      console.log(`   Current: ${currentVersion}`);
      return;
    }

    console.log('');
    console.log(`🆕 New version available: ${latestRelease.tag_name}`);
    console.log(`   Current: ${currentVersion}`);
    console.log('');

    // 自动安装最新版本
    console.log('🚀 Starting update...');
    await install({ version: latestRelease.tag_name });

  } catch (error) {
    console.error('❌ Update failed:', error.message);
    if (error.stack) {
      console.error(error.stack);
    }
    process.exit(1);
  }
}

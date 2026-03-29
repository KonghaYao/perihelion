#!/usr/bin/env node

import { Command } from 'commander';
import { install } from '../src/commands/install.js';
import { listVersions } from '../src/commands/list.js';
import { update } from '../src/commands/update.js';
import { addEnv } from '../src/commands/add-env.js';
import { uninstall } from '../src/commands/uninstall.js';

const program = new Command();

program
  .name('peri')
  .description('Perihelion Rust Agent Framework CLI')
  .version('0.1.0');

program
  .command('install', { isDefault: true })
  .description('Install or update Perihelion to the latest version')
  .option('-v, --version <version>', 'Install specific version')
  .action(install);

program
  .command('list')
  .alias('ls')
  .description('List available versions on GitHub (top 5)')
  .action(listVersions);

program
  .command('update')
  .description('Update Perihelion to the latest version')
  .action(update);

program
  .command('add-env')
  .description('Add Perihelion binary to your PATH (shell config)')
  .action(addEnv);

program
  .command('uninstall')
  .description('Uninstall peri and clean up')
  .action(uninstall);

program.parse();

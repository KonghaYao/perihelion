#!/usr/bin/env node

import { Command } from "commander";
import { install } from "../src/commands/install.js";
import { listVersions } from "../src/commands/list.js";
import { update } from "../src/commands/update.js";
import { addEnv } from "../src/commands/add-env.js";
import { uninstall } from "../src/commands/uninstall.js";
import { clean } from "../src/commands/clean.js";

const program = new Command();

program
    .name("peri")
    .description("Peri Rust Agent Framework CLI")
    .version("0.1.0");

program
    .command("install [package]")
    .description(
        "Install a package (e.g. 'agent', or full tag 'agent-v1.17')",
    )
    .action(install);

program
    .command("list")
    .alias("ls")
    .description("List available versions on GitHub (top 5)")
    .action(listVersions);

program
    .command("update [package]", { isDefault: true })
    .description(
        "Update a package to the latest version (e.g. 'agent')",
    )
    .action(update);

program
    .command("add-env")
    .description("Add Peri binary to your PATH (shell config)")
    .action(addEnv);

program
    .command("uninstall")
    .description("Uninstall peri and clean up")
    .action(uninstall);

program
    .command("clean")
    .description("Remove old versions, keep latest 2 per package")
    .action(clean);

program.parse();

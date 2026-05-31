use anyhow::{Context, Result};
use git2::Oid;
use std::path::Path;
use std::process::Command;

use super::repo::GitRepo;

impl GitRepo {
    /// 从 git 索引读取暂存区文件内容
    pub fn read_staged_file(&self, path: &str) -> Result<Vec<u8>> {
        let index = self.repo().index()?;
        let entry = index
            .get_path(Path::new(path), 0)
            .with_context(|| format!("文件不在索引中: {}", path))?;
        let blob = self.repo().find_blob(entry.id)?;
        Ok(blob.content().to_vec())
    }

    /// 从工作区读取文件内容
    pub fn read_working_file(&self, path: &str) -> Result<Vec<u8>> {
        let full = self.workdir()?.join(path);
        std::fs::read(&full).with_context(|| format!("读取文件失败: {}", full.display()))
    }
    pub fn checkout(&self, oid: Oid) -> Result<()> {
        let short = format!("{:.7}", oid);
        self.run_git(&["checkout", &short])
    }

    /// 按分支名 checkout
    pub fn checkout_branch(&self, name: &str) -> Result<()> {
        self.run_git(&["checkout", name])
    }

    pub fn merge(&self, oid: Oid) -> Result<()> {
        let short = format!("{:.7}", oid);
        self.run_git(&["merge", &short])
    }

    pub fn cherry_pick(&self, oid: Oid) -> Result<()> {
        let short = format!("{:.7}", oid);
        self.run_git(&["cherry-pick", &short])
    }

    #[allow(dead_code)]
    pub fn reset_soft(&self, oid: Oid) -> Result<()> {
        let short = format!("{:.7}", oid);
        self.run_git(&["reset", "--soft", &short])
    }

    pub fn reset_hard(&self, oid: Oid) -> Result<()> {
        let short = format!("{:.7}", oid);
        self.run_git(&["reset", "--hard", &short])
    }

    #[allow(dead_code)]
    pub fn create_tag(&self, oid: Oid, name: &str) -> Result<()> {
        let short = format!("{:.7}", oid);
        self.run_git(&["tag", name, &short])
    }

    pub fn create_branch(&self, oid: Oid, name: &str) -> Result<()> {
        let short = format!("{:.7}", oid);
        self.run_git(&["branch", name, &short])
    }

    pub fn delete_tag(&self, name: &str) -> Result<()> {
        self.run_git(&["tag", "-d", name])
    }

    pub fn push_tag(&self, name: &str) -> Result<()> {
        self.run_git(&["push", "origin", name])
    }

    pub fn delete_branch(&self, name: &str) -> Result<()> {
        self.run_git(&["branch", "-D", name])
    }

    /// 将文件添加到暂存区（git add <path>）
    pub fn stage_file(&self, path: &str) -> Result<()> {
        self.run_git(&["add", path])
    }

    /// 将文件从暂存区移回工作区（git restore --staged <path>）
    pub fn unstage_file(&self, path: &str) -> Result<()> {
        self.run_git(&["restore", "--staged", path])
    }

    /// 丢弃工作区修改（git restore <path>）
    #[allow(dead_code)]
    pub fn discard_file(&self, path: &str) -> Result<()> {
        self.run_git(&["restore", path])
    }

    /// 删除已跟踪文件的修改并删除文件（git rm <path>）
    pub fn delete_tracked_file(&self, path: &str) -> Result<()> {
        self.run_git(&["rm", "-f", path])
    }

    /// 删除未跟踪文件（直接 rm）
    pub fn delete_untracked_file(&self, path: &str) -> Result<()> {
        let full = self.workdir()?.join(path);
        if full.exists() {
            if full.is_dir() {
                std::fs::remove_dir_all(&full)?;
            } else {
                std::fs::remove_file(&full)?;
            }
        }
        Ok(())
    }

    /// 丢弃目录的所有变更（git restore + git clean -fd）
    /// 不删除目录本身，只丢弃修改和清理未跟踪文件
    pub fn discard_dir_changes(&self, path: &str) -> Result<()> {
        let dir_path = path.trim_end_matches('/');
        // 先丢弃已跟踪文件的修改
        let _ = self.run_git_allow_fail(&["restore", dir_path]);
        // 清理未跟踪文件和目录
        self.run_git(&["clean", "-fd", dir_path])
    }

    fn workdir(&self) -> Result<&std::path::Path> {
        self.repo().workdir().context("bare 仓库不支持此操作")
    }

    fn run_git(&self, args: &[&str]) -> Result<()> {
        let workdir = self.workdir()?;
        let output = Command::new("git")
            .args(args)
            .current_dir(workdir)
            .output()
            .with_context(|| format!("执行 git {} 失败", args.join(" ")))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("git {} 失败: {}", args.join(" "), stderr);
        }
        Ok(())
    }

    /// 执行 git 命令但忽略错误（用于 restore 等可能无变更的命令）
    fn run_git_allow_fail(&self, args: &[&str]) -> Result<()> {
        let workdir = self.workdir()?;
        let output = Command::new("git")
            .args(args)
            .current_dir(workdir)
            .output()
            .with_context(|| format!("执行 git {} 失败", args.join(" ")))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            tracing::debug!("git {}（允许失败）: {}", args.join(" "), stderr);
        }
        Ok(())
    }
}

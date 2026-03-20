use std::path::{Path, PathBuf};

use gray_matter::{engine::YAML, Matter};
use serde::Deserialize;

/// Skill 元数据（来自 SKILL.md frontmatter）
#[derive(Debug, Clone)]
pub struct SkillMetadata {
    pub name: String,
    pub description: String,
    pub path: PathBuf,
}

/// frontmatter 反序列化结构
#[derive(Debug, Deserialize)]
struct SkillFrontmatter {
    name: String,
    description: String,
}

/// 加载单个 SKILL.md，解析 frontmatter，返回元数据
pub fn load_skill_metadata(path: &Path) -> Option<SkillMetadata> {
    let content = std::fs::read_to_string(path).ok()?;
    let matter = Matter::<YAML>::new();
    let result = matter.parse(&content);

    let data = result.data?;
    let fm: SkillFrontmatter = data.deserialize().ok()?;

    Some(SkillMetadata {
        name: fm.name,
        description: fm.description,
        path: path.to_path_buf(),
    })
}

/// 扫描多个目录，返回所有可用 skill 元数据
///
/// 同名 skill 以先出现的为准（dirs 中靠前的目录优先）。
pub fn list_skills(dirs: &[PathBuf]) -> Vec<SkillMetadata> {
    let mut seen_names = std::collections::HashSet::new();
    let mut skills = Vec::new();

    for dir in dirs {
        if !dir.is_dir() {
            continue;
        }

        // 遍历直接子目录，每个子目录中寻找 SKILL.md
        let entries = match std::fs::read_dir(dir) {
            Ok(e) => e,
            Err(_) => continue,
        };

        let mut dir_skills: Vec<SkillMetadata> = entries
            .filter_map(|entry| {
                let entry = entry.ok()?;
                let path = entry.path();
                if path.is_dir() {
                    let skill_file = path.join("SKILL.md");
                    if skill_file.is_file() {
                        return load_skill_metadata(&skill_file);
                    }
                } else if path.is_file()
                    && path.file_name().map(|n| n == "SKILL.md").unwrap_or(false)
                {
                    return load_skill_metadata(&path);
                }
                None
            })
            .collect();

        // 按名称排序保持稳定顺序
        dir_skills.sort_by(|a, b| a.name.cmp(&b.name));

        for skill in dir_skills {
            if seen_names.insert(skill.name.clone()) {
                skills.push(skill);
            }
        }
    }

    skills
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn write_skill(dir: &Path, name: &str, desc: &str) {
        let skill_dir = dir.join(name);
        std::fs::create_dir_all(&skill_dir).unwrap();
        let content = format!(
            "---\nname: '{}'\ndescription: '{}'\n---\n\n# {}\n\nContent here.\n",
            name, desc, name
        );
        std::fs::write(skill_dir.join("SKILL.md"), content).unwrap();
    }

    #[test]
    fn test_load_skill_metadata() {
        let dir = tempdir().unwrap();
        write_skill(dir.path(), "my-skill", "A test skill");
        let skill_file = dir.path().join("my-skill").join("SKILL.md");
        let meta = load_skill_metadata(&skill_file).unwrap();
        assert_eq!(meta.name, "my-skill");
        assert_eq!(meta.description, "A test skill");
    }

    #[test]
    fn test_list_skills_dedup() {
        let dir1 = tempdir().unwrap();
        let dir2 = tempdir().unwrap();
        write_skill(dir1.path(), "skill-a", "from dir1");
        write_skill(dir1.path(), "skill-b", "from dir1");
        write_skill(dir2.path(), "skill-a", "from dir2"); // 重复，应被忽略
        write_skill(dir2.path(), "skill-c", "from dir2");

        let skills = list_skills(&[dir1.path().to_path_buf(), dir2.path().to_path_buf()]);
        assert_eq!(skills.len(), 3);

        let skill_a = skills.iter().find(|s| s.name == "skill-a").unwrap();
        assert_eq!(skill_a.description, "from dir1"); // dir1 优先
    }
}

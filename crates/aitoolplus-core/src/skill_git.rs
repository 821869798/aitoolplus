//! Git-backed Skill installation/update for the central repository.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::paths::Paths;
use crate::skills::{Skill, SkillsStore, central_repo_path, upsert};

fn validate_url(url: &str) -> Result<(), String> {
    let url = url.trim();
    if !(url.starts_with("https://") || url.starts_with("http://") || url.starts_with("git@")) {
        return Err("Git source must use http(s) or git@ URL".into());
    }
    if url.contains('\n') || url.contains('\r') {
        return Err("invalid Git source".into());
    }
    Ok(())
}

fn name_from_url(url: &str) -> String {
    url.trim_end_matches('/')
        .rsplit(['/', ':'])
        .next()
        .unwrap_or("skill")
        .trim_end_matches(".git")
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '-' || character == '_' {
                character
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_string()
}

fn run_git(args: &[&str]) -> Result<(), String> {
    let output = Command::new("git")
        .args(args)
        .output()
        .map_err(|error| format!("git unavailable: {error}"))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}

pub fn install(
    store: &mut SkillsStore,
    paths: &Paths,
    url: &str,
    requested_name: Option<&str>,
) -> Result<String, String> {
    validate_url(url)?;
    let name = requested_name
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(String::from)
        .unwrap_or_else(|| name_from_url(url));
    if name.is_empty() {
        return Err("could not derive Skill name from Git URL".into());
    }
    let repo = central_repo_path(&store.settings, paths);
    std::fs::create_dir_all(&repo).map_err(|error| error.to_string())?;
    let target = repo.join(&name);
    if target.exists() {
        return Err(format!("Skill already exists: {name}"));
    }
    let target_string = target.to_string_lossy().to_string();
    run_git(&["clone", "--depth", "1", url, &target_string])?;
    if !target.join("SKILL.md").is_file() && !target.join("skill.md").is_file() {
        let _ = std::fs::remove_dir_all(&target);
        return Err("cloned repository has no root SKILL.md".into());
    }
    let mut skill = Skill::new(&name, &name);
    skill.source_type = "git".into();
    skill.source_ref = Some(url.into());
    upsert(store, skill);
    Ok(name)
}

pub fn update(store: &mut SkillsStore, paths: &Paths, id: &str) -> Result<(), String> {
    let skill = store
        .skills
        .iter_mut()
        .find(|skill| skill.id == id)
        .ok_or("Skill not found")?;
    if skill.source_type != "git" {
        return Err("Skill is not Git-backed".into());
    }
    let directory = central_repo_path(&store.settings, paths).join(&skill.central_path);
    let directory_string = directory.to_string_lossy().to_string();
    run_git(&["-C", &directory_string, "pull", "--ff-only", "--depth", "1"])?;
    skill.updated_at = chrono::Utc::now().timestamp_millis();
    skill.status = "ok".into();
    Ok(())
}

pub fn is_git_repository(path: &Path) -> bool {
    path.join(".git").is_dir()
}

pub fn repository_path(store: &SkillsStore, paths: &Paths, id: &str) -> Option<PathBuf> {
    let skill = store.skills.iter().find(|skill| skill.id == id)?;
    Some(central_repo_path(&store.settings, paths).join(&skill.central_path))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_validation_and_name() {
        assert!(validate_url("https://github.com/acme/my-skill.git").is_ok());
        assert!(validate_url("file:///tmp/skill").is_err());
        assert!(validate_url("https://x\n--upload-pack=bad").is_err());
        assert_eq!(
            name_from_url("https://github.com/acme/my-skill.git"),
            "my-skill"
        );
        assert_eq!(name_from_url("git@github.com:acme/x.git"), "x");
    }

    #[test]
    fn local_git_update_flow() {
        let temp = tempfile::tempdir().unwrap();
        let remote = temp.path().join("remote");
        std::fs::create_dir_all(&remote).unwrap();
        std::fs::write(remote.join("SKILL.md"), "# v1\n").unwrap();
        run_git(&["-C", remote.to_str().unwrap(), "init"]).unwrap();
        run_git(&[
            "-C",
            remote.to_str().unwrap(),
            "config",
            "user.email",
            "test@example.com",
        ])
        .unwrap();
        run_git(&[
            "-C",
            remote.to_str().unwrap(),
            "config",
            "user.name",
            "Test",
        ])
        .unwrap();
        run_git(&["-C", remote.to_str().unwrap(), "add", "SKILL.md"]).unwrap();
        run_git(&["-C", remote.to_str().unwrap(), "commit", "-m", "initial"]).unwrap();

        // Production validation intentionally rejects file://; this test uses
        // git clone directly to create the target, then exercises update.
        let paths = Paths::new(temp.path().join("home"), temp.path().join("data"));
        let mut store = SkillsStore::default();
        let target = central_repo_path(&store.settings, &paths).join("git-skill");
        std::fs::create_dir_all(target.parent().unwrap()).unwrap();
        run_git(&["clone", remote.to_str().unwrap(), target.to_str().unwrap()]).unwrap();
        let mut skill = Skill::new("git-skill", "git-skill");
        skill.source_type = "git".into();
        skill.source_ref = Some("https://example.com/git-skill.git".into());
        let id = skill.id.clone();
        upsert(&mut store, skill);
        assert!(is_git_repository(&target));
        update(&mut store, &paths, &id).unwrap();
    }
}

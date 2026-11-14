use anyhow::{Context, Result, bail};
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::mpsc::Sender,
    thread,
};

use crate::app::AppEvent;

/// List files directly inside `dir` whose extension matches one of `extensions`.
/// Returns an empty list (not an error) if the directory doesn't exist yet.
pub fn discover_files(dir: &Path, extensions: &[&str]) -> Result<Vec<PathBuf>> {
    if !dir.exists() {
        return Ok(Vec::new());
    }

    let mut result = Vec::new();
    for entry in fs::read_dir(dir).with_context(|| format!("cannot read {}", dir.display()))? {
        let path = entry?.path();
        if path.is_file()
            && path
                .extension()
                .and_then(|x| x.to_str())
                .is_some_and(|ext| extensions.contains(&ext))
        {
            result.push(path);
        }
    }
    result.sort();
    Ok(result)
}

/// List the immediate children of `host_vars/`: either a `<host>.yml` file or a
/// `<host>/` directory containing var files. Both layouts are valid Ansible conventions.
pub fn discover_host_vars(dir: &Path) -> Result<Vec<PathBuf>> {
    if !dir.exists() {
        return Ok(Vec::new());
    }

    let mut result = Vec::new();
    for entry in fs::read_dir(dir).with_context(|| format!("cannot read {}", dir.display()))? {
        let path = entry?.path();
        let name_ok = path
            .file_name()
            .and_then(|s| s.to_str())
            .is_some_and(|s| !s.starts_with('.'));
        if !name_ok {
            continue;
        }

        let is_var_file = path
            .extension()
            .and_then(|x| x.to_str())
            .is_some_and(|ext| ext == "yml" || ext == "yaml" || ext == "json");
        if path.is_dir() || is_var_file {
            result.push(path);
        }
    }
    result.sort();
    Ok(result)
}

/// Render the preview text for a host_vars entry: raw content for a file, or the
/// concatenation of every var file inside the directory (each under its own header).
pub fn read_host_vars_preview(path: &Path) -> String {
    if path.is_dir() {
        let mut files = match discover_files(path, &["yml", "yaml", "json"]) {
            Ok(files) => files,
            Err(err) => return format!("cannot read {}: {err:#}", path.display()),
        };
        files.sort();

        if files.is_empty() {
            return "(empty - no var files in this directory)".into();
        }

        let mut out = String::new();
        for (i, file) in files.iter().enumerate() {
            if i > 0 {
                out.push('\n');
            }
            let name = file.file_name().and_then(|s| s.to_str()).unwrap_or("?");
            out.push_str(&format!("# {name}\n"));
            match fs::read_to_string(file) {
                Ok(text) => out.push_str(&text),
                Err(err) => out.push_str(&format!("<cannot read: {err}>")),
            }
            if !out.ends_with('\n') {
                out.push('\n');
            }
        }
        out
    } else {
        fs::read_to_string(path)
            .unwrap_or_else(|err| format!("cannot read {}: {err}", path.display()))
    }
}

/// Run `ansible-playbook <playbook> --list-tags` and collect the tags that
/// identify a play (or, for untagged plays, its tasks). This makes the playbook
/// the single source of truth for "what can I run" instead of a separate,
/// hand-maintained folder of files.
pub fn list_playbook_tags(
    root: &Path,
    playbook: &Path,
    inventory: Option<&Path>,
) -> Result<Vec<String>> {
    let mut cmd = Command::new("ansible-playbook");
    cmd.arg(playbook)
        .arg("--list-tags")
        .current_dir(root)
        // Never let a missing vault password or similar prompt hang the TUI.
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    if let Some(inv) = inventory {
        cmd.arg("-i").arg(inv);
    }

    let output = cmd
        .output()
        .context("could not start ansible-playbook; is Ansible installed?")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let last_line = stderr
            .lines()
            .rev()
            .find(|l| !l.trim().is_empty())
            .unwrap_or("no output");
        bail!(
            "ansible-playbook --list-tags failed ({}): {}",
            output.status,
            last_line.trim()
        );
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(parse_list_tags_output(&stdout))
}

/// Split an Ansible tag list such as `[a, b, c]` into its individual tags.
fn parse_tag_list(raw: &str) -> Vec<String> {
    let inner = raw.trim().trim_start_matches('[').trim_end_matches(']');
    inner
        .split(',')
        .map(str::trim)
        .filter(|tag| !tag.is_empty())
        .map(str::to_string)
        .collect()
}

/// Parse the output of `--list-tags`.
///
/// Each play is printed as a `play #N ... TAGS: [...]` header followed by a
/// `TASK TAGS: [...]` line that aggregates the tags of every task in the play.
/// The play-level tags are the actual "actions" of the playbook: a play tagged
/// `update_prune_docker` whose imported tasks also carry `pull`/`restart`/`prune`
/// must stay a single entry, not four. So prefer the play tags and only fall
/// back to the task tags for plays that don't declare any of their own.
fn parse_list_tags_output(text: &str) -> Vec<String> {
    let mut plays: Vec<(Vec<String>, Vec<String>)> = Vec::new();

    for line in text.lines() {
        let trimmed = line.trim();

        if trimmed.starts_with("play #") {
            // Play names may themselves contain "TAGS:", so split on the last one.
            let play_tags = trimmed
                .rsplit("TAGS:")
                .next()
                .map(parse_tag_list)
                .unwrap_or_default();
            plays.push((play_tags, Vec::new()));
        } else if let Some(rest) = trimmed.strip_prefix("TASK TAGS:")
            && let Some((_, task_tags)) = plays.last_mut()
        {
            task_tags.extend(parse_tag_list(rest));
        }
    }

    let mut tags = BTreeSet::new();
    for (play_tags, task_tags) in plays {
        if play_tags.is_empty() {
            tags.extend(task_tags);
        } else {
            tags.extend(play_tags);
        }
    }
    tags.into_iter().collect()
}

/// Parse an INI-style Ansible inventory and return the sorted, de-duplicated host list.
/// Group headers (`[web]`), comments and `key=value` variable lines are skipped.
pub fn parse_inventory_hosts(path: &Path) -> Result<Vec<String>> {
    let text =
        fs::read_to_string(path).with_context(|| format!("cannot read {}", path.display()))?;

    let mut hosts = Vec::new();
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty()
            || line.starts_with('#')
            || line.starts_with(';')
            || line.starts_with('[')
        {
            continue;
        }
        if let Some(first) = line.split_whitespace().next()
            && !first.contains('=')
            && !first.starts_with('-')
        {
            hosts.push(first.to_string());
        }
    }
    hosts.sort();
    hosts.dedup();
    Ok(hosts)
}

/// Parse a line of `ansible-playbook` output such as `ok: [web1]` or
/// `changed: [web2] => {...}` into (status, host, trailing detail).
pub fn parse_host_line(line: &str) -> Option<(&str, String, &str)> {
    for status in [
        "ok",
        "changed",
        "failed",
        "unreachable",
        "skipped",
        "rescued",
        "ignored",
    ] {
        let prefix = format!("{status}: [");
        if let Some(start) = line.find(&prefix) {
            let rest = &line[start + prefix.len()..];
            if let Some(end) = rest.find(']') {
                let host = rest[..end].to_string();
                let detail = rest[end + 1..].trim();
                return Some((status, host, detail));
            }
        }
    }
    None
}

/// Spawn `ansible-playbook`, streaming stdout/stderr lines back as [`AppEvent`]s
/// and finishing with the process exit code.
pub fn run_ansible(
    root: &Path,
    playbook: &Path,
    tag: &str,
    inventory: &Path,
    tx: Sender<AppEvent>,
) -> Result<()> {
    let mut child = Command::new("ansible-playbook")
        .arg(playbook)
        .arg("--tags")
        .arg(tag)
        .arg("-i")
        .arg(inventory)
        .current_dir(root)
        .env("ANSIBLE_FORCE_COLOR", "0")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("could not start ansible-playbook; is Ansible installed?")?;

    let stdout = child.stdout.take().context("missing stdout")?;
    let stderr = child.stderr.take().context("missing stderr")?;

    let tx_out = tx.clone();
    thread::spawn(move || {
        use std::io::{BufRead, BufReader};
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            let _ = tx_out.send(AppEvent::Output(format!("{line}\n")));
        }
    });

    {
        use std::io::{BufRead, BufReader};
        for line in BufReader::new(stderr).lines().map_while(Result::ok) {
            let _ = tx.send(AppEvent::Output(format!("{line}\n")));
        }
    }

    let status = child.wait().context("failed waiting for ansible")?;
    let code = status.code().unwrap_or(1);
    let _ = tx.send(AppEvent::Finished(code));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_list_tags_output_across_multiple_plays() {
        let sample = "\
playbook: playbook.yaml

  play #1 (all): Web servers\tTAGS: []
      TASK TAGS: [deploy, restart]

  play #2 (db): Database servers\tTAGS: []
      TASK TAGS: [backup, deploy, migrate]
";
        let tags = parse_list_tags_output(sample);
        assert_eq!(tags, vec!["backup", "deploy", "migrate", "restart"]);
    }

    #[test]
    fn parses_empty_tags_as_no_tags() {
        let sample =
            "playbook: playbook.yaml\n\n  play #1 (all): x\tTAGS: []\n      TASK TAGS: []\n";
        assert!(parse_list_tags_output(sample).is_empty());
    }

    #[test]
    fn prefers_play_tags_over_tags_of_imported_tasks() {
        // Regression: a play tagged `update_prune_docker` imports tasks that also
        // declare `pull`, `restart` and `prune`. Only the play tag is an action.
        let sample = "\
playbook: playbook.yaml

  play #1 (debian): Update Debian container\tTAGS: [update_debian]
      TASK TAGS: [update_debian]

  play #3 (docker): Update Docker Images\tTAGS: [update_prune_docker]
      TASK TAGS: [prune, pull, restart, update_prune_docker]
";
        let tags = parse_list_tags_output(sample);
        assert_eq!(tags, vec!["update_debian", "update_prune_docker"]);
    }

    #[test]
    fn falls_back_to_task_tags_when_play_has_none() {
        let sample = "\
playbook: playbook.yaml

  play #1 (all): untagged play\tTAGS: []
      TASK TAGS: [deploy, restart]

  play #2 (db): tagged play\tTAGS: [migrate]
      TASK TAGS: [backup, migrate]
";
        let tags = parse_list_tags_output(sample);
        assert_eq!(tags, vec!["deploy", "migrate", "restart"]);
    }

    #[test]
    fn play_name_containing_the_word_tags_does_not_confuse_the_parser() {
        let sample = "\
playbook: playbook.yaml

  play #1 (all): manage TAGS: everywhere\tTAGS: [manage]
      TASK TAGS: [manage]
";
        assert_eq!(parse_list_tags_output(sample), vec!["manage"]);
    }
}

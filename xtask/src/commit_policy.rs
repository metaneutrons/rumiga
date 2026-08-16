// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2025 Fabian Schmieder

use std::env;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{Context, Result, bail, ensure};

use super::{capture_git, workspace_root};

const ALLOWED_TYPES: [&str; 11] = [
    "build", "chore", "ci", "docs", "feat", "fix", "perf", "refactor", "revert", "style", "test",
];
const BASE_ENVIRONMENT_VARIABLE: &str = "RUMIGA_COMMIT_BASE";
const HEAD_ENVIRONMENT_VARIABLE: &str = "RUMIGA_COMMIT_HEAD";
const PR_TITLE_ENVIRONMENT_VARIABLE: &str = "RUMIGA_PR_TITLE";
const MAX_HEADER_CHARACTERS: usize = 120;
const MAX_MESSAGE_BYTES: usize = 64 * 1024;

#[derive(Debug, Eq, PartialEq)]
enum CliAction {
    MessageFile(PathBuf),
    Range { base: String, head: String },
    Repository,
    Help,
}

pub fn run(arguments: &[String]) -> Result<()> {
    match parse_arguments(arguments)? {
        CliAction::MessageFile(path) => {
            let message = read_message_file(&path)?;
            validate_message(&message)?;
            println!("commit policy: message is valid");
            Ok(())
        }
        CliAction::Range { base, head } => {
            let root = workspace_root()?;
            validate_repository_range(&root, Some(&base), &head)
        }
        CliAction::Repository => {
            let root = workspace_root()?;
            validate_repository(&root)
        }
        CliAction::Help => {
            print_help();
            Ok(())
        }
    }
}

pub fn validate_repository(root: &Path) -> Result<()> {
    let configured_base = environment_value(BASE_ENVIRONMENT_VARIABLE);
    let configured_head = environment_value(HEAD_ENVIRONMENT_VARIABLE);
    if configured_base.is_some() || configured_head.is_some() {
        let head = configured_head.with_context(|| {
            format!(
                "{HEAD_ENVIRONMENT_VARIABLE} is required when {BASE_ENVIRONMENT_VARIABLE} is set"
            )
        })?;
        let base = configured_base.filter(|value| !is_zero_object_id(value));
        validate_repository_range(root, base.as_deref(), &head)?;
    } else {
        let head = resolve_commit(root, "HEAD")?;
        let base = resolve_optional_commit(root, "origin/main")?
            .map(|main| merge_base(root, &main, &head))
            .transpose()?
            .filter(|candidate| candidate != &head);
        validate_repository_range(root, base.as_deref(), &head)?;
    }

    if let Some(title) = environment_value(PR_TITLE_ENVIRONMENT_VARIABLE) {
        validate_message(&title).context("pull-request title violates commit policy")?;
        println!("commit policy: pull-request title is valid");
    }
    Ok(())
}

fn parse_arguments(arguments: &[String]) -> Result<CliAction> {
    match arguments {
        [] => Ok(CliAction::Repository),
        [help] if help == "--help" || help == "-h" => Ok(CliAction::Help),
        [option, path] if option == "--message-file" => {
            ensure!(!path.is_empty(), "--message-file requires a path");
            Ok(CliAction::MessageFile(PathBuf::from(path)))
        }
        [option, range] if option == "--range" => {
            let (base, head) = parse_range(range)?;
            Ok(CliAction::Range { base, head })
        }
        _ => bail!("invalid commit-policy options; use --help for usage"),
    }
}

fn parse_range(range: &str) -> Result<(String, String)> {
    let (base, head) = range
        .split_once("..")
        .context("--range must use the form <base>..<head>")?;
    ensure!(
        !base.is_empty() && !head.is_empty() && !head.contains(".."),
        "--range must contain exactly two non-empty object IDs"
    );
    validate_object_id(base, "range base")?;
    validate_object_id(head, "range head")?;
    Ok((base.to_owned(), head.to_owned()))
}

fn print_help() {
    println!(
        "Rumiga commit policy\n\n\
         Usage:\n  cargo +1.97.1 xtask commit-policy\n  \
         cargo +1.97.1 xtask commit-policy --message-file <path>\n  \
         cargo +1.97.1 xtask commit-policy --range <base>..<head>\n\n\
         With no options, the repository range is selected from the CI environment\n\
         or from the local origin/main merge base."
    );
}

fn read_message_file(path: &Path) -> Result<String> {
    let metadata = fs::metadata(path)
        .with_context(|| format!("failed to inspect commit message file {}", path.display()))?;
    ensure!(
        metadata.is_file(),
        "commit message path is not a regular file: {}",
        path.display()
    );
    ensure!(
        metadata.len() <= MAX_MESSAGE_BYTES as u64,
        "commit message exceeds {MAX_MESSAGE_BYTES} bytes"
    );
    fs::read_to_string(path)
        .with_context(|| format!("commit message is not valid UTF-8: {}", path.display()))
}

fn validate_message(message: &str) -> Result<()> {
    ensure!(
        message.len() <= MAX_MESSAGE_BYTES,
        "commit message exceeds {MAX_MESSAGE_BYTES} bytes"
    );
    ensure!(
        !message.contains('\0'),
        "commit message contains a NUL byte"
    );
    ensure!(
        !message.contains('\r'),
        "commit message must use LF line endings"
    );
    ensure!(
        !message
            .chars()
            .any(|character| { character.is_control() && !matches!(character, '\n' | '\t') }),
        "commit message contains an unsupported control character"
    );

    let mut lines = message
        .lines()
        .filter(|line| !line.starts_with('#'))
        .collect::<Vec<_>>();
    while lines.last().is_some_and(|line| line.is_empty()) {
        lines.pop();
    }
    let header = lines.first().context("commit message is empty")?;
    validate_header(header)?;
    if lines.len() > 1 {
        ensure!(
            lines[1].is_empty(),
            "commit body must be separated from the header by a blank line"
        );
    }
    Ok(())
}

fn validate_header(header: &str) -> Result<()> {
    ensure!(!header.is_empty(), "commit header is empty");
    ensure!(
        header.trim() == header,
        "commit header has leading or trailing whitespace"
    );
    ensure!(
        header.chars().count() <= MAX_HEADER_CHARACTERS,
        "commit header exceeds {MAX_HEADER_CHARACTERS} characters"
    );

    let lowercase = header.to_ascii_lowercase();
    ensure!(
        !lowercase.starts_with("fixup!")
            && !lowercase.starts_with("squash!")
            && !lowercase.starts_with("amend!")
            && lowercase != "wip"
            && !lowercase.starts_with("wip:")
            && !lowercase.starts_with("wip "),
        "autosquash and WIP commits are not allowed"
    );

    let (prefix, description) = header
        .split_once(": ")
        .context("commit header must use <type>(<scope>)!: <description>")?;
    ensure!(
        !description.is_empty() && description.trim() == description,
        "commit description must be non-empty and trimmed"
    );
    ensure!(
        !description.chars().any(char::is_control),
        "commit description contains a control character"
    );

    let prefix = prefix.strip_suffix('!').unwrap_or(prefix);
    ensure!(
        !prefix.contains('!'),
        "breaking marker must appear once immediately before the colon"
    );
    let (commit_type, scope) = if let Some((commit_type, remainder)) = prefix.split_once('(') {
        let scope = remainder
            .strip_suffix(')')
            .context("commit scope must end with a closing parenthesis")?;
        ensure!(
            !commit_type.is_empty() && !scope.contains(['(', ')']),
            "commit scope has invalid parentheses"
        );
        (commit_type, Some(scope))
    } else {
        ensure!(
            !prefix.contains(['(', ')']),
            "commit scope must use balanced parentheses"
        );
        (prefix, None)
    };

    ensure!(
        ALLOWED_TYPES.contains(&commit_type),
        "unsupported commit type {commit_type:?}; allowed types: {}",
        ALLOWED_TYPES.join(", ")
    );
    if let Some(scope) = scope {
        ensure!(valid_scope(scope), "invalid commit scope {scope:?}");
    }
    Ok(())
}

fn valid_scope(scope: &str) -> bool {
    let bytes = scope.as_bytes();
    !bytes.is_empty()
        && bytes
            .first()
            .is_some_and(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
        && bytes
            .last()
            .is_some_and(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
        && bytes.iter().all(|byte| {
            byte.is_ascii_lowercase()
                || byte.is_ascii_digit()
                || matches!(byte, b'-' | b'_' | b'.' | b'/')
        })
}

fn validate_repository_range(root: &Path, base: Option<&str>, head: &str) -> Result<()> {
    validate_object_id(head, "head")?;
    let resolved_head = resolve_commit(root, head)?;
    let resolved_base = base
        .map(|value| {
            validate_object_id(value, "base")?;
            let resolved = resolve_commit(root, value)?;
            merge_base(root, &resolved, &resolved_head)
        })
        .transpose()?;

    let commits = if let Some(base) = &resolved_base {
        let range = format!("{base}..{resolved_head}");
        capture_git(root, &["rev-list", "--reverse", "--topo-order", &range])?
    } else {
        resolved_head.clone()
    };
    let commits = commits
        .lines()
        .filter(|line| !line.is_empty())
        .map(str::to_owned)
        .collect::<Vec<_>>();
    ensure!(!commits.is_empty(), "commit range contains no commits");

    let mut failures = String::new();
    for commit in &commits {
        let (parent_count, message) = read_commit_object(root, commit)?;
        if parent_count > 1 {
            let _ = writeln!(
                failures,
                "  {}: merge commits are not allowed",
                short_object_id(commit)
            );
            continue;
        }
        if let Err(error) = validate_message(&message) {
            let subject = message.lines().next().unwrap_or("<empty>");
            let _ = writeln!(
                failures,
                "  {} {subject:?}: {error:#}",
                short_object_id(commit)
            );
        }
    }
    ensure!(
        failures.is_empty(),
        "commit policy rejected the selected range:\n{failures}"
    );

    let range = resolved_base.map_or_else(
        || resolved_head.clone(),
        |base| format!("{base}..{resolved_head}"),
    );
    println!(
        "commit policy: {} commit(s) valid in {range}",
        commits.len()
    );
    Ok(())
}

fn read_commit_object(root: &Path, commit: &str) -> Result<(usize, String)> {
    let output = Command::new("git")
        .current_dir(root)
        .args(["cat-file", "commit", commit])
        .stdin(Stdio::null())
        .output()
        .with_context(|| format!("failed to read Git commit {commit}"))?;
    ensure!(
        output.status.success(),
        "git cat-file failed for commit {}: {}",
        short_object_id(commit),
        String::from_utf8_lossy(&output.stderr).trim()
    );
    let object = String::from_utf8(output.stdout)
        .with_context(|| format!("commit {} is not valid UTF-8", short_object_id(commit)))?;
    let (headers, message) = object.split_once("\n\n").with_context(|| {
        format!(
            "commit {} has no message separator",
            short_object_id(commit)
        )
    })?;
    let parent_count = headers
        .lines()
        .filter(|line| line.starts_with("parent "))
        .count();
    Ok((parent_count, message.to_owned()))
}

fn environment_value(name: &str) -> Option<String> {
    env::var(name).ok().filter(|value| !value.trim().is_empty())
}

fn validate_object_id(value: &str, label: &str) -> Result<()> {
    ensure!(
        matches!(value.len(), 40 | 64) && value.bytes().all(|byte| byte.is_ascii_hexdigit()),
        "{label} must be a full hexadecimal Git object ID"
    );
    Ok(())
}

fn is_zero_object_id(value: &str) -> bool {
    matches!(value.len(), 40 | 64) && value.bytes().all(|byte| byte == b'0')
}

fn resolve_commit(root: &Path, revision: &str) -> Result<String> {
    let commit = format!("{revision}^{{commit}}");
    let resolved = capture_git(root, &["rev-parse", "--verify", &commit])?;
    validate_object_id(&resolved, "resolved commit")?;
    Ok(resolved)
}

fn resolve_optional_commit(root: &Path, revision: &str) -> Result<Option<String>> {
    let commit = format!("{revision}^{{commit}}");
    let output = Command::new("git")
        .current_dir(root)
        .args(["rev-parse", "--verify", &commit])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .with_context(|| format!("failed to resolve optional Git revision {revision}"))?;
    if !output.status.success() {
        return Ok(None);
    }
    let resolved = String::from_utf8(output.stdout)
        .context("git returned a non-UTF-8 object ID")?
        .trim()
        .to_owned();
    validate_object_id(&resolved, "resolved commit")?;
    Ok(Some(resolved))
}

fn merge_base(root: &Path, base: &str, head: &str) -> Result<String> {
    let resolved = capture_git(root, &["merge-base", base, head])?;
    validate_object_id(&resolved, "merge base")?;
    Ok(resolved)
}

fn short_object_id(value: &str) -> &str {
    value.get(..12).unwrap_or(value)
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::process::{Command, Stdio};
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::{
        CliAction, parse_arguments, parse_range, valid_scope, validate_message,
        validate_repository_range,
    };

    static NEXT_REPOSITORY_ID: AtomicU64 = AtomicU64::new(0);

    struct TestRepository {
        path: PathBuf,
    }

    impl TestRepository {
        fn new() -> Self {
            let id = NEXT_REPOSITORY_ID.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir()
                .join(format!("rumiga-commit-policy-{}-{id}", std::process::id()));
            fs::create_dir(&path).expect("test repository directory must be created");
            run_git(&path, &["init", "--quiet"]);
            Self { path }
        }

        fn empty_tree(&self) -> String {
            run_git(&self.path, &["mktree"])
        }

        fn commit(&self, tree: &str, parents: &[&str], message: &str) -> String {
            let mut command = git_command(&self.path);
            command.args(["commit-tree", tree]);
            for parent in parents {
                command.args(["-p", parent]);
            }
            command.args(["-m", message]);
            run_git_command(&mut command)
        }
    }

    impl Drop for TestRepository {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    fn strings(values: &[&str]) -> Vec<String> {
        values.iter().map(ToString::to_string).collect()
    }

    #[test]
    fn accepts_supported_conventional_commit_forms() {
        for message in [
            "feat(core): add bounded event queue",
            "fix(display)!: correct viewport origin\n\nBREAKING CHANGE: viewport coordinates are native pixels",
            "revert: restore the previous runtime profile\n\nThis reverts commit 0123456789abcdef.",
            "chore(deps): bump locked Rust dependencies",
            "chore(deps-web): bump locked web dependencies",
            "chore(ci-deps): bump pinned actions",
            "docs: explain the policy\n\nRefs: M0-013",
        ] {
            validate_message(message).unwrap_or_else(|error| {
                panic!("expected valid message {message:?}, got {error:#}")
            });
        }
    }

    #[test]
    fn rejects_unsupported_or_malformed_headers() {
        for message in [
            "add a feature",
            "WIP",
            "fixup! feat(core): add queue",
            "squash! feat(core): add queue",
            "feature(core): add queue",
            "feat(Core): add queue",
            "feat(core-): add queue",
            "feat((core)): add queue",
            "feat(core):",
            "feat(core) : add queue",
            "feat(core): add queue\nbody without separator",
        ] {
            assert!(
                validate_message(message).is_err(),
                "expected invalid message: {message:?}"
            );
        }
    }

    #[test]
    fn rejects_oversized_or_unsafe_messages() {
        let long_header = format!("feat(core): {}", "x".repeat(121));
        assert!(validate_message(&long_header).is_err());
        assert!(validate_message("feat(core): add queue\r\n").is_err());
        assert!(validate_message("feat(core): add\0queue").is_err());
    }

    #[test]
    fn accepts_only_bounded_lowercase_scopes() {
        for scope in ["core", "deps-web", "m68k", "api/v1", "ci_deps"] {
            assert!(valid_scope(scope));
        }
        for scope in ["", "Core", "-core", "core-", "core space", "core@api"] {
            assert!(!valid_scope(scope));
        }
    }

    #[test]
    fn cli_requires_full_object_ids_and_unambiguous_modes() {
        let base = "0".repeat(40);
        let head = "1".repeat(40);
        assert_eq!(
            parse_arguments(&strings(&["--range", &format!("{base}..{head}")]))
                .expect("range must parse"),
            CliAction::Range { base, head }
        );
        assert!(parse_range("main..HEAD").is_err());
        assert!(parse_arguments(&strings(&["--message-file"])).is_err());
        assert!(parse_arguments(&strings(&["--message-file", "message", "extra"])).is_err());
    }

    #[test]
    fn repository_range_rejects_invalid_messages_and_merge_commits() {
        let repository = TestRepository::new();
        let tree = repository.empty_tree();
        let base = repository.commit(&tree, &[], "chore: establish test history");
        let valid = repository.commit(&tree, &[&base], "feat(core): add deterministic state");
        validate_repository_range(&repository.path, Some(&base), &valid)
            .expect("valid linear range must pass");

        let invalid = repository.commit(&tree, &[&valid], "WIP");
        let error = validate_repository_range(&repository.path, Some(&base), &invalid)
            .expect_err("invalid raw commit message must fail");
        assert!(error.to_string().contains("autosquash and WIP"));

        let side = repository.commit(&tree, &[&base], "test(core): add side fixture");
        let merge = repository.commit(
            &tree,
            &[&valid, &side],
            "chore(history): combine test branches",
        );
        let error = validate_repository_range(&repository.path, Some(&base), &merge)
            .expect_err("merge commit must fail");
        assert!(error.to_string().contains("merge commits are not allowed"));
    }

    fn git_command(path: &Path) -> Command {
        let mut command = Command::new("git");
        command
            .current_dir(path)
            .env("GIT_AUTHOR_NAME", "Rumiga Test")
            .env("GIT_AUTHOR_EMAIL", "test@rumiga.invalid")
            .env("GIT_COMMITTER_NAME", "Rumiga Test")
            .env("GIT_COMMITTER_EMAIL", "test@rumiga.invalid")
            .stdin(Stdio::null());
        command
    }

    fn run_git(path: &Path, arguments: &[&str]) -> String {
        let mut command = git_command(path);
        command.args(arguments);
        run_git_command(&mut command)
    }

    fn run_git_command(command: &mut Command) -> String {
        let output = command.output().expect("git command must start");
        assert!(
            output.status.success(),
            "git command failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout)
            .expect("git output must be UTF-8")
            .trim()
            .to_owned()
    }
}

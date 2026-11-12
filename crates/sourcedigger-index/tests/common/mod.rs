use std::path::Path;
use std::process::Command;

pub fn git(directory: &Path, arguments: &[&str]) {
    let status = Command::new("git")
        .args([
            "-c",
            "user.name=test",
            "-c",
            "user.email=test@localhost",
            "-c",
            "init.defaultBranch=main",
        ])
        .args(arguments)
        .current_dir(directory)
        .status()
        .expect("git is installed");
    assert!(status.success(), "git {arguments:?} failed");
}

pub fn create_repository(directory: &Path, versions: &[(&str, &[(&str, &str)])]) {
    git(directory, &["init", "--quiet"]);
    for (tag, files) in versions {
        git(directory, &["rm", "-r", "--quiet", "--ignore-unmatch", "."]);
        for (path, content) in *files {
            let path = directory.join(path);
            std::fs::create_dir_all(path.parent().expect("files are inside the repository"))
                .expect("creating directory");
            std::fs::write(path, content).expect("writing file");
        }
        git(directory, &["add", "--all"]);
        git(
            directory,
            &["commit", "--quiet", "--allow-empty", "--message", tag],
        );
        git(directory, &["tag", tag]);
    }
}

use anyhow::{Context, Result, anyhow, ensure};
use serde::Deserialize;
use std::collections::HashSet;
use std::fs::copy;
use std::path::PathBuf;
use std::process::Command;

fn main() -> Result<()> {
    let args: Vec<_> = std::env::args_os().collect();
    let is_help = args
        .iter()
        .any(|arg| arg == "-h" || arg == "-?" || arg == "-help" || arg == "--help");
    if is_help {
        println!("USAGE: cargo update-target-dylibs [--help|--release]");
        println!();
        println!("Copies dynamic libraries built for dependencies into the target directory.");
        println!("Cargo workspaces are supported, but a particular package must be specified;");
        println!("the simplest way to specify a package is to run in that package's directory.");
        println!();
        println!("Specify additional arguments for `cargo build` with the environment variables");
        println!("CARGO_ARGS and/or CARGO_BUILD_ARGS. As a convenience, the --release flag is");
        println!("passed through to `cargo build`.");
        return Ok(());
    }
    let mut cmd = Command::new("cargo");
    cmd.arg("pkgid");
    let pkg_id = get_cmd(cmd).context("running `cargo pkgid`")?;
    let is_release = std::env::args_os().any(|arg| arg == "--release");
    let mut cmd = Command::new("cargo");
    cmd.arg("build");
    for key in ["CARGO_ARGS", "CARGO_BUILD_ARGS"] {
        if let Ok(addl_args) = std::env::var(key) {
            cmd.args(addl_args.split_ascii_whitespace());
        }
    }
    if is_release {
        cmd.arg("--release");
    }
    cmd.args(["--quiet", "--message-format", "json"]);
    let build_messages = get_cmd(cmd).context("running `cargo build`")?;
    let mut pkg_message = None;
    let mut libraries = Vec::new();
    for (i, line) in build_messages.lines().enumerate() {
        let message = serde_json::from_str::<BuildMessage>(line)
            .context(format!("`cargo build` message {}", i))?;
        if message.reason == "compiler-artifact" && message.package_id.as_deref() == Some(&pkg_id) {
            pkg_message = Some(message);
            continue;
        }
        if message.reason != "build-script-executed"
            || message.package_id.is_none()
            || message.linked_libs.is_none()
        {
            continue;
        }
        let package_id = message.package_id.unwrap();
        let linked_libs = message.linked_libs.unwrap();
        if linked_libs.len() == 0 {
            continue;
        }
        let linked_paths = message.linked_paths.ok_or(anyhow!(
            "missing paths for libraries in package '{}'",
            package_id
        ))?;
        ensure!(
            linked_paths.len() != 0,
            "no paths for libraries in package '{}'",
            package_id
        );
        let paths = linked_paths.into_iter().collect::<HashSet<_>>();
        for name in linked_libs.into_iter() {
            libraries.push(Library {
                name,
                paths: paths.clone(),
            });
        }
    }
    let pkg_message = pkg_message.ok_or(anyhow!(
        "no 'compiler-artifact' message found for package '{}'",
        pkg_id
    ))?;
    let target_path = PathBuf::from({
        if let Some(executable) = pkg_message.executable {
            executable
        } else {
            let filenames = pkg_message
                .filenames
                .ok_or(anyhow!("missing filenames for package '{}'", pkg_id))?;
            let rlib_file = filenames
                .into_iter()
                .find(|elem| elem.ends_with(".rlib"))
                .ok_or(anyhow!("missing rlib file for package '{}'", pkg_id))?;
            rlib_file
        }
    });
    let target_path = target_path
        .parent()
        .ok_or(anyhow!("can't find parent path for package '{}'", pkg_id))?;
    for Library { name, paths } in libraries {
        let lib_name = format!("{}{}{}", DYLIB_PREFIX, name, DYLIB_SUFFIX);
        for path in paths.into_iter() {
            let src_path = PathBuf::from(path);
            let src_path = src_path
                .parent()
                .ok_or(anyhow!("no parent path for library '{}'", &lib_name))?;
            let src_path = src_path.join("bin").join(&lib_name);
            if !src_path.exists() {
                continue;
            }
            let dst_path = PathBuf::from(target_path).join(&lib_name);
            copy(&src_path, &dst_path).context(format!("copying library '{}'", lib_name))?;
            println!("{} -> {}", src_path.display(), dst_path.display());
        }
    }
    Ok(())
}

#[derive(Deserialize)]
struct BuildMessage {
    reason: String,
    package_id: Option<String>,
    linked_libs: Option<Vec<String>>,
    linked_paths: Option<Vec<String>>,
    executable: Option<String>,
    filenames: Option<Vec<String>>,
}

struct Library {
    name: String,
    paths: HashSet<String>,
}

fn get_cmd(mut cmd: Command) -> Result<String> {
    let output = cmd.output().context("executing command")?;
    ensure!(
        output.status.success(),
        "exited with status {}",
        output.status
    );
    let mut string = String::from_utf8(output.stdout).context("converting stdout to string")?;
    string.truncate(string.trim_ascii_end().len());
    Ok(string)
}

const DYLIB_PREFIX: &'static str = if cfg!(target_os = "windows") {
    ""
} else {
    "lib"
};
const DYLIB_SUFFIX: &'static str = if cfg!(target_os = "windows") {
    ".dll"
} else if cfg!(target_os = "macos") {
    ".dylib"
} else {
    ".so"
};

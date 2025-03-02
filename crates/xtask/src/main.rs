#![allow(dead_code)]

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand, ValueEnum};
use xshell::{Shell, cmd};

#[derive(Parser)]
struct Args {
    #[clap(subcommand)]
    subcommand: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Generates markdown files for all the CLIs in the codebase.
    Clipages,
    /// Generates manpages for all the CLIs in the codebase.
    Manpages,
    /// Format the codebase
    Fmt {
        /// Check if the codebase is formatted rather than formatting it
        #[clap(short, long)]
        check: bool,
    },
    /// Lint the codebase with clippy
    Lint,
    /// Checks for unused dependencies in the codebase
    UDeps {
        /// Fix unused dependencies
        #[clap(short, long)]
        fix: bool,
    },

    /// Install's UPX only on linux.
    #[cfg(target_os = "linux")]
    InstallUpx,
    /// Builds the project
    Build(Build),
    /// For watching the project and auto-rebuilding
    Dev { target: BuildTarget },
}

#[derive(Parser)]
struct Build {
    /// Uses UPX to compress the binary.
    #[clap(long, short)]
    compress: bool,
    /// The profile to build with.
    #[clap(short, long, default_value = "release-big")]
    profile: Profile,
    /// Use cranelift as the compiler.
    #[clap(long)]
    cranelift: bool,
    /// Only applies to aot.
    #[clap(long)]
    cuda: bool,
    /// The linker to use for compilation.
    #[clap(long, short, default_value = "default")]
    linker: Linker,
    /// The target binary to build.
    target: BuildTarget,
}

impl Build {
    fn run_inner(&self, sh: &Shell, package: &str) -> Result<()> {
        let profile = self.profile.as_ref();

        // if crane lift is enabled, we need to build with nightly
        let cmd = if self.cranelift || matches!(self.linker, Linker::RustLld) {
            cmd!(sh, "cargo +nightly build")
        } else {
            cmd!(sh, "cargo build")
        };
        let cmd = cmd.arg("--profile").arg(profile).arg("-p").arg(package);

        let cmd = if self.cuda && matches!(self.target, BuildTarget::Aot) {
            cmd.arg("--features").arg("cuda")
        } else {
            cmd
        };

        // if cranelift is enabled we need to set the env var, and also specify the
        // target
        let mut env_flags = self.linker.as_ref().to_string();
        let cmd = if self.cranelift {
            // -C panic=abort
            env_flags.push_str(" -C lto=no -Zlocation-detail=none -Zcodegen-backend=cranelift -C target-cpu=native");
            // This is broken >.<
            // // if cranelift is enabled, and the target is not AOT, we can pass additional
            // // flags
            // if !matches!(self.target, BuildTarget::Aot) {
            //     env_flags.push_str(
            //         " -Zbuild-std=std,panic_abort
            // -Zbuild-std-features=panic_immediate_abort",     );
            // }

            cmd.env("RUSTFLAGS", env_flags)
                .arg("--target")
                .arg("x86_64-unknown-linux-gnu")
        } else {
            cmd.env("RUSTFLAGS", env_flags)
        };

        cmd.run()?;

        if self.compress {
            let profile = if profile == "dev" { "debug" } else { profile };

            cmd!(sh, "upx --best -f --lzma -o ./target/{package}-compressed ./target/x86_64-unknown-linux-gnu/{profile}/{package}").run()?;
        }

        Ok(())
    }
    fn run(self, sh: &Shell) -> Result<()> {
        self.linker.check_installed(sh)?;

        if matches!(self.target, BuildTarget::All) {
            self.run_inner(sh, "snops-agent")?;
            self.run_inner(sh, "snops-cli")?;
            self.run_inner(sh, "snops")?;
            self.run_inner(sh, "snarkos-aot")?;
        } else {
            self.run_inner(sh, self.target.as_ref())?;
        }

        Ok(())
    }
}

#[derive(Clone, ValueEnum)]
enum BuildTarget {
    All,
    Agent,
    Aot,
    Cli,
    #[clap(alias = "cp")]
    ControlPlane,
}

impl AsRef<str> for BuildTarget {
    fn as_ref(&self) -> &str {
        match self {
            BuildTarget::All => "",
            BuildTarget::Aot => "snarkos-aot",
            BuildTarget::ControlPlane => "snops",
            BuildTarget::Agent => "snops-agent",
            BuildTarget::Cli => "snops-cli",
        }
    }
}

#[derive(Clone, ValueEnum, Default)]
enum Linker {
    #[default]
    Default,
    Lld,
    Mold,
    RustLld,
}

impl Linker {
    fn check_installed(&self, sh: &Shell) -> Result<()> {
        match self {
            Linker::Default => Ok(()),
            Linker::Lld => {
                if cmd!(sh, "which ld.lld").read().is_err() {
                    bail!("lld is not installed, please install it")
                }
                Ok(())
            }
            Linker::Mold => {
                if cmd!(sh, "which mold").read().is_err() {
                    bail!("mold is not installed, please install it")
                }
                Ok(())
            }
            Linker::RustLld => Ok(()),
        }
    }
}

impl AsRef<str> for Linker {
    fn as_ref(&self) -> &str {
        match self {
            Linker::Default => "",
            Linker::Lld => "-C link-arg=-fuse-ld=lld",
            Linker::Mold => "-C linker=clang -C link-arg=-fuse-ld=mold",
            Linker::RustLld => "-Zlinker-features=-lld",
        }
    }
}

#[derive(Clone, ValueEnum)]
enum Profile {
    ReleaseBig,
    ReleaseSmall,
    Dev,
}

impl AsRef<str> for Profile {
    fn as_ref(&self) -> &str {
        match self {
            Profile::ReleaseBig => "release-big",
            Profile::ReleaseSmall => "release-small",
            Profile::Dev => "dev",
        }
    }
}

impl Command {
    fn run(self, sh: &Shell) -> Result<()> {
        match self {
            Command::Clipages => clipages(sh),
            Command::Manpages => manpages(sh),
            Command::Fmt { check } => fmt(sh, check),
            Command::Lint => cmd!(
                sh,
                "cargo +nightly clippy --all-targets --all-features -- -D warnings"
            )
            .run()
            .context("Running clippy"),
            Command::UDeps { fix } => udeps(sh, fix),
            #[cfg(target_os = "linux")]
            Command::InstallUpx => install_upx(sh),
            Command::Build(build) => build.run(sh),
            Command::Dev { target } => dev(sh, target),
        }
    }
}

fn main() {
    if let Err(e) = try_main() {
        eprintln!("{e}");
        std::process::exit(1);
    }
}

fn try_main() -> Result<()> {
    let sh = Shell::new()?;

    // Ensure our working directory is the toplevel
    {
        let path = cmd!(&sh, "git rev-parse --show-toplevel")
            .read()
            .context("Failed to invoke git rev-parse")?;
        std::env::set_current_dir(path.trim()).context("Changing to toplevel")?;
    }

    let args = Args::parse();

    args.subcommand.run(&sh)?;

    Ok(())
}

fn clipages(sh: &Shell) -> Result<()> {
    cmd!(sh, "cargo run -p snarkos-aot --features=docpages -- md").run()?;
    cmd!(sh, "cargo run -p snops --features=docpages -- md").run()?;
    cmd!(
        sh,
        "cargo run -p snops-agent --features=docpages -- --id foo md"
    )
    .run()?;
    cmd!(sh, "cargo run -p snops-cli --features=docpages -- md").run()?;
    Ok(())
}

fn manpages(sh: &Shell) -> Result<()> {
    cmd!(sh, "cargo run -p snarkos-aot --features=docpages -- man").run()?;
    cmd!(sh, "cargo run -p snops --features=docpages -- man").run()?;
    cmd!(
        sh,
        "cargo run -p snops-agent --features=docpages -- --id foo man"
    )
    .run()?;
    cmd!(sh, "cargo run -p snops-cli --features=docpages -- man").run()?;
    Ok(())
}

fn fmt(sh: &Shell, check: bool) -> Result<()> {
    let cmd = cmd!(sh, "cargo +nightly fmt --all");
    let cmd = if check { cmd.arg("-- --check") } else { cmd };

    cmd.run()?;
    Ok(())
}

fn install_cargo_subcommands(sh: &Shell, subcmd: &'static str) -> Result<()> {
    cmd!(sh, "cargo install {subcmd} --locked").run()?;
    Ok(())
}

fn udeps(sh: &Shell, fix: bool) -> Result<()> {
    install_cargo_subcommands(sh, "cargo-shear")?;
    let cmd = cmd!(sh, "cargo-shear");
    let cmd = if fix { cmd.arg("--fix") } else { cmd };
    cmd.run()?;
    Ok(())
}

#[cfg(target_os = "linux")]
fn install_upx(sh: &Shell) -> Result<()> {
    // Check if upx is already installed and return early if it is
    if !cmd!(sh, "which upx").read()?.is_empty() {
        return Ok(());
    }

    cmd!(
        sh,
        "wget https://github.com/upx/upx/releases/download/v4.2.3/upx-4.2.3-amd64_linux.tar.xz"
    )
    .run()?;
    cmd!(sh, "tar -xf upx-4.2.3-amd64_linux.tar.xz").run()?;
    cmd!(sh, "cp ./upx-4.2.3-amd64_linux/upx /usr/local/bin/").run()?;
    cmd!(
        sh,
        "rm -rf upx-4.2.3-amd64_linux.tar.xz upx-4.2.3-amd64_linux/"
    )
    .run()?;
    Ok(())
}

fn dev(sh: &Shell, target: BuildTarget) -> Result<()> {
    install_cargo_subcommands(sh, "cargo-watch")?;

    match target {
        BuildTarget::All => cmd!(
            sh,
            "cargo watch -x 'build --profile release-big' -w ./crates/agent -w ./crates/common -w ./crates/checkpoint -w ./crates/controlplane -w ./crates/cli -w ./crates/aot"
        )
        .run(),
        BuildTarget::Agent => cmd!(
            sh,
            "cargo watch -x 'build -p snops-agent --profile release-big' -w ./crates/agent -w ./crates/common -w ./crates/checkpoint"
        )
        .run(),
        BuildTarget::Aot => cmd!(
            sh,
            "cargo watch -x 'build -p snarkos-aot --profile release-big' -w ./crates/aot"
        )
        .run(),
        BuildTarget::Cli => cmd!(
            sh,
            "cargo watch -x 'build -p snops-cli' -w ./crates/cli"
        )
        .run(),
        BuildTarget::ControlPlane => {
            cmd!(sh, "cargo watch -x 'run -p snops' -w ./crates/controlplane -w ./crates/common -w ./crates/checkpoint").run()
        }

    }?;

    Ok(())
}

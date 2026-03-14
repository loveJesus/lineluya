// For God so loved the world that he gave his only begotten Son,
// that whoever believes in him should not perish but have eternal life. - John 3:16

//! Lineluya kernel build tool (xtask pattern).
//!
//! Orchestrates building the kernel, creating disk images, and running QEMU.
//! The `bootloader` crate's BIOS stage compilation requires a Linux host, so
//! disk image creation is deferred to Linux. On macOS, we build and verify
//! the kernel ELF compiles correctly.

use std::env;
use std::path::PathBuf;
use std::process::{self, Command};

/// The target triple for the bare-metal x86_64 kernel.
const TARGET_TRIPLE_CHIRHO: &str = "x86_64-unknown-none";

/// Default memory size for the QEMU virtual machine.
const QEMU_MEMORY_CHIRHO: &str = "512M";

fn main() {
    main_chirho();
}

fn main_chirho() {
    let args_chirho: Vec<String> = env::args().collect();
    let subcommand_chirho = args_chirho.get(1).map(|s_chirho| s_chirho.as_str());

    match subcommand_chirho {
        Some("build") => {
            build_kernel_chirho(false);
            println!("Kernel ELF built at: {}", kernel_binary_path_chirho(false).display());
        }
        Some("build-release") => {
            build_kernel_chirho(true);
            println!("Kernel ELF built at: {}", kernel_binary_path_chirho(true).display());
        }
        Some("run") => {
            build_kernel_chirho(false);
            let kernel_path_chirho = kernel_binary_path_chirho(false);
            run_qemu_chirho(&kernel_path_chirho);
        }
        Some("run-release") => {
            build_kernel_chirho(true);
            let kernel_path_chirho = kernel_binary_path_chirho(true);
            run_qemu_chirho(&kernel_path_chirho);
        }
        Some("check") => {
            check_kernel_chirho();
        }
        Some("clippy") => {
            clippy_kernel_chirho();
        }
        Some("info") => {
            build_kernel_chirho(false);
            show_kernel_info_chirho(false);
        }
        Some(unknown_chirho) => {
            eprintln!("Error: unknown subcommand '{}'", unknown_chirho);
            print_usage_chirho();
            process::exit(1);
        }
        None => {
            eprintln!("Error: no subcommand provided");
            print_usage_chirho();
            process::exit(1);
        }
    }
}

fn print_usage_chirho() {
    eprintln!();
    eprintln!("Lineluya Kernel Build Tool");
    eprintln!();
    eprintln!("Usage: cargo run --package xtask-chirho -- <COMMAND>");
    eprintln!();
    eprintln!("Commands:");
    eprintln!("  build          Build the kernel (debug)");
    eprintln!("  build-release  Build the kernel (release, optimized)");
    eprintln!("  run            Build and run in QEMU");
    eprintln!("  run-release    Build (release) and run in QEMU");
    eprintln!("  check          Check the kernel compiles without codegen");
    eprintln!("  clippy         Run clippy lints on the kernel");
    eprintln!("  info           Show kernel binary info (sections, size)");
}

fn workspace_root_chirho() -> PathBuf {
    let manifest_dir_chirho =
        PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".to_string()));

    let candidate_chirho = manifest_dir_chirho.join("kernel-chirho");
    if candidate_chirho.exists() {
        manifest_dir_chirho
    } else {
        manifest_dir_chirho
            .parent()
            .expect("Cannot determine workspace root")
            .to_path_buf()
    }
}

fn kernel_binary_path_chirho(release_chirho: bool) -> PathBuf {
    let root_chirho = workspace_root_chirho();
    let profile_chirho = if release_chirho { "release" } else { "debug" };
    root_chirho
        .join("target")
        .join(TARGET_TRIPLE_CHIRHO)
        .join(profile_chirho)
        .join("kernel-chirho")
}

fn build_kernel_chirho(release_chirho: bool) {
    let root_chirho = workspace_root_chirho();

    println!(
        "Building kernel-chirho ({})...",
        if release_chirho { "release" } else { "debug" }
    );

    let mut cmd_chirho = Command::new("cargo");
    cmd_chirho
        .arg("+nightly")
        .arg("build")
        .arg("--package")
        .arg("kernel-chirho")
        .current_dir(&root_chirho);

    if release_chirho {
        cmd_chirho.arg("--release");
    }

    let status_chirho = cmd_chirho.status().unwrap_or_else(|err_chirho| {
        panic!("Failed to execute cargo build: {}", err_chirho);
    });

    if !status_chirho.success() {
        eprintln!("Kernel build failed");
        process::exit(1);
    }

    println!("Kernel build successful.");
}

fn check_kernel_chirho() {
    let root_chirho = workspace_root_chirho();

    let status_chirho = Command::new("cargo")
        .arg("+nightly")
        .arg("check")
        .arg("--package")
        .arg("kernel-chirho")
        .current_dir(&root_chirho)
        .status()
        .unwrap_or_else(|err_chirho| {
            panic!("Failed to execute cargo check: {}", err_chirho);
        });

    if !status_chirho.success() {
        process::exit(1);
    }
    println!("Kernel check passed.");
}

fn clippy_kernel_chirho() {
    let root_chirho = workspace_root_chirho();

    let status_chirho = Command::new("cargo")
        .arg("+nightly")
        .arg("clippy")
        .arg("--package")
        .arg("kernel-chirho")
        .arg("--")
        .arg("-D")
        .arg("warnings")
        .current_dir(&root_chirho)
        .status()
        .unwrap_or_else(|err_chirho| {
            panic!("Failed to execute cargo clippy: {}", err_chirho);
        });

    if !status_chirho.success() {
        process::exit(1);
    }
    println!("Clippy passed.");
}

fn show_kernel_info_chirho(release_chirho: bool) {
    let kernel_path_chirho = kernel_binary_path_chirho(release_chirho);
    if !kernel_path_chirho.exists() {
        eprintln!("Kernel binary not found. Run build first.");
        process::exit(1);
    }

    println!("=== Kernel Binary Info ===");
    println!("Path: {}", kernel_path_chirho.display());

    // Try rust-objdump for section info
    let _ = Command::new("rust-objdump")
        .arg("-h")
        .arg(&kernel_path_chirho)
        .status();

    // Show size
    let _ = Command::new("rust-size")
        .arg(&kernel_path_chirho)
        .status();
}

/// Run the kernel in QEMU.
///
/// Uses `-kernel` to load the ELF directly. For a fully bootable image,
/// the kernel needs Multiboot2 or Linux boot protocol headers.
/// For now, we use the bootloader_api protocol — in production, we'll
/// create proper disk images on a Linux host.
fn run_qemu_chirho(kernel_path_chirho: &PathBuf) {
    if !kernel_path_chirho.exists() {
        eprintln!("Kernel binary not found at: {}", kernel_path_chirho.display());
        process::exit(1);
    }

    println!("Launching QEMU with kernel: {}", kernel_path_chirho.display());
    println!("Note: Serial output goes to stdio. Press Ctrl+A then X to quit QEMU.");

    let status_chirho = Command::new("qemu-system-x86_64")
        .arg("-kernel")
        .arg(kernel_path_chirho)
        .arg("-serial")
        .arg("stdio")
        .arg("-display")
        .arg("none")
        .arg("-m")
        .arg(QEMU_MEMORY_CHIRHO)
        .arg("-no-reboot")
        .arg("-no-shutdown")
        .status()
        .unwrap_or_else(|err_chirho| {
            panic!(
                "Failed to launch qemu-system-x86_64: {}.\n\
                 Install QEMU: brew install qemu",
                err_chirho
            );
        });

    if !status_chirho.success() {
        let code_chirho = status_chirho.code().map_or("signal".to_string(), |c| c.to_string());
        eprintln!("QEMU exited with code: {}", code_chirho);
    }
}

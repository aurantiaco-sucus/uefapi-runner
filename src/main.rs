use std::env::args;
use std::fs;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use log::{error, info, warn};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub struct RunnerConfig {
    pub project_path: String,
    pub auto_build: bool,
    pub build_cmd: String,
    pub binary_path: String,
    pub efi_name: String,
    pub move_binary: bool,
    pub qemu_cmd: String,
    pub ovmf_path: String,
    pub stdio_serial: bool,
    pub log_serial: bool,
    pub log_path: String,
}

pub fn example() -> RunnerConfig {
    RunnerConfig {
        project_path: ".".to_string(),
        auto_build: true,
        build_cmd: "build --target x86_64-unknown-uefi --release".to_string(),
        binary_path: "target/x86_64-unknown-uefi/debug/your_bin_name.efi".to_string(),
        efi_name: "BOOTX64.EFI".to_string(),
        move_binary: true,
        qemu_cmd: "/path_to_qemu/qemu-system-x86_64".to_string(),
        ovmf_path: "/path_to_ovmf_files".to_string(),
        stdio_serial: true,
        log_serial: true,
        log_path: "runner-x86_64-release.log".to_string(),
    }
}

fn main() {
    env_logger::init();
    info!("UEFAPI Cargo UEFI Project Runner, Version {}", env!("CARGO_PKG_VERSION"));
    if let Some("gen") = args().nth(1).as_deref() {
        let config = example();
        let config = toml::to_string_pretty(&config)
            .expect("Failed to serialize example config");
        fs::write("uefapi-runner.toml", config)
            .expect("Failed to write example config");
        info!("Example config written to uefapi-runner.toml");
        return;
    }
    let config_path = args().nth(1).unwrap_or("uefapi-runner.toml".to_string());
    info!("Loading config from {}", config_path);
    let config = fs::read_to_string(config_path)
        .expect("Failed to read config file");
    let config: RunnerConfig = toml::from_str(&config)
        .expect("Failed to parse config file");
    info!("Config loaded: {:?}", config);
    if !config.auto_build && config.move_binary {
        warn!("Moving binary away but not auto-building, this may cause issues");
    }
    if config.auto_build {
        info!("Building project");
        let mut cmd = Command::new("cargo")
            .args(config.build_cmd.split_whitespace())
            .current_dir(config.project_path)
            .stdout(Stdio::inherit())
            .spawn().expect("Failed to run build command");
        let status = cmd.wait().expect("Failed to wait for build command");
        if !status.success() {
            error!("Build failed");
            return;
        }
        info!("Build successful");
    }
    let work_dir = tempfile::tempdir().expect("Failed to create temp dir");
    let work_dir = work_dir.path();
    let efi_boot_dir = work_dir.join("EFI/BOOT");
    fs::create_dir_all(&efi_boot_dir)
        .expect("Failed to create EFI/BOOT directory");
    let efi_bin_path = efi_boot_dir.join(&config.efi_name);
    if config.move_binary {
        info!("Moving binary to {}", efi_bin_path.display());
        fs::rename(&config.binary_path, &efi_bin_path)
            .expect("Failed to move binary");
    } else {
        info!("Copying binary to {}", efi_bin_path.display());
        fs::copy(&config.binary_path, &efi_bin_path)
            .expect("Failed to copy binary");
    }
    let ovmf_path = PathBuf::from(&config.ovmf_path).canonicalize()
        .expect("Failed to canonicalize OVMF path");
    let ovmf_code_bin = ovmf_path.join("OVMF_CODE.fd");
    let ovmf_vars_bin = ovmf_path.join("OVMF_VARS.fd");
    if !ovmf_code_bin.exists() || !ovmf_vars_bin.exists() {
        error!("OVMF files not found in path");
        info!("Hint: This tool needs OVMF_CODE.fd and OVMF_VARS.fd to run");
        return;
    }
    let mut cmd = Command::new(&config.qemu_cmd);
    let mut cmd = cmd
        .args(["-machine", "q35"])
        .arg("-drive")
        .arg(format!("if=pflash,format=raw,file={}", ovmf_code_bin.display()))
        .arg("-drive")
        .arg(format!("if=pflash,format=raw,file={}", ovmf_vars_bin.display()))
        .arg("-drive")
        .arg(format!("format=raw,file=fat:rw:{}", work_dir.display()));
    if config.stdio_serial {
        cmd = cmd
            .arg("-chardev")
            .arg(format!("{}id=char0,logfile={}", 
                         if config.stdio_serial { "stdio," } else { "" },
                         config.log_path))
            .args(["-serial", "chardev:char0"]);
    }
    let mut child = cmd.spawn().expect("Failed to run QEMU");
    info!("QEMU started");
    let status = child.wait().expect("Failed to wait for QEMU");
    info!("QEMU exited with status: {}", status);
}

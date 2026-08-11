//! Performs the installation and reports each visible stage to the interface.

use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::process::CommandExt;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::thread;
use std::time::Duration;

const MIN_STORAGE_GIB: u64 = 20;
const PACKAGE_LIST: &str = include_str!("../packages.txt");
const FLUFFSETUP_SUDOERS_PATH: &str = "/etc/sudoers.d/90-fluffsetup-temporary";
const FLUFFSETUP_DESKTOP_SOURCE: &str = "/usr/lib/fluffinstall/fluffsetup/fluffsetup.desktop";
const FLUFFSETUP_SESSION_SOURCE: &str = "/usr/lib/fluffinstall/fluffsetup/fluffsetup-session";
const FLUFFSETUP_BINARY_SOURCE: &str = "/usr/lib/fluffinstall/fluffsetup/fluffsetup";

#[derive(Clone, Debug)]
pub struct Progress {
    pub overall: i32,
    pub completed: i32,
    pub total: i32,
    pub status: String,
    pub detail: String,
}

fn report(
    callback: &mut impl FnMut(Progress),
    overall: i32,
    completed: i32,
    total: i32,
    status: &str,
) {
    callback(Progress {
        overall,
        completed,
        total,
        status: status.to_string(),
        detail: String::new(),
    });
}

fn report_with_detail(
    callback: &mut impl FnMut(Progress),
    overall: i32,
    completed: i32,
    total: i32,
    status: &str,
    detail: &str,
) {
    callback(Progress {
        overall,
        completed,
        total,
        status: status.to_string(),
        detail: detail.to_string(),
    });
}

const CANCELLED_MESSAGE: &str = "Installation cancelled.";
// Only one installation can run at a time. These values let the interface
// stop the complete privileged process group instead of only its first child.
static INSTALLATION_CANCELLABLE: AtomicBool = AtomicBool::new(false);
static INSTALLATION_CANCEL_REQUESTED: AtomicBool = AtomicBool::new(false);
static ACTIVE_INSTALLATION_PROCESS_GROUP: AtomicU32 = AtomicU32::new(0);

fn check_cancelled(cancel_requested: &AtomicBool) -> Result<(), String> {
    if cancel_requested.load(Ordering::Relaxed) {
        Err(CANCELLED_MESSAGE.to_string())
    } else {
        Ok(())
    }
}

fn validate_fluffsetup_session_files() -> Result<(), String> {
    let missing_files = [
        FLUFFSETUP_DESKTOP_SOURCE,
        FLUFFSETUP_SESSION_SOURCE,
        FLUFFSETUP_BINARY_SOURCE,
    ]
    .into_iter()
    .filter(|path| !fs::metadata(path).is_ok_and(|metadata| metadata.is_file()))
    .collect::<Vec<_>>();

    if missing_files.is_empty() {
        Ok(())
    } else {
        eprintln!(
            "Missing pre-installation files: {}",
            missing_files.join(", ")
        );
        Err("Installation files are missing, the drive was not formatted.".to_string())
    }
}

fn privileged_program(command: &str) -> String {
    format!("/usr/bin/{command}")
}

fn track_privileged_child(child: &Child) {
    let process_group_id = child.id();
    if INSTALLATION_CANCELLABLE.load(Ordering::Acquire) {
        ACTIVE_INSTALLATION_PROCESS_GROUP.store(process_group_id, Ordering::Release);
        if INSTALLATION_CANCEL_REQUESTED.load(Ordering::Acquire) {
            force_kill_process_group(process_group_id);
        }
    }
}

fn clear_tracked_child(process_group_id: u32) {
    let _ = ACTIVE_INSTALLATION_PROCESS_GROUP.compare_exchange(
        process_group_id,
        0,
        Ordering::AcqRel,
        Ordering::Acquire,
    );
}

fn wait_for_privileged_child(child: &mut Child) -> std::io::Result<ExitStatus> {
    let process_group_id = child.id();
    track_privileged_child(child);
    let result = child.wait();
    clear_tracked_child(process_group_id);
    result
}

pub(crate) fn request_installation_cancellation() {
    INSTALLATION_CANCEL_REQUESTED.store(true, Ordering::Release);
}

pub(crate) fn cancel_active_installation_process() {
    request_installation_cancellation();
    let process_group_id = ACTIVE_INSTALLATION_PROCESS_GROUP.load(Ordering::Acquire);
    if process_group_id != 0 {
        force_kill_process_group(process_group_id);
    }
}

fn begin_cancellable_installation(cancel_already_requested: bool) {
    ACTIVE_INSTALLATION_PROCESS_GROUP.store(0, Ordering::Release);
    INSTALLATION_CANCEL_REQUESTED.store(cancel_already_requested, Ordering::Release);
    INSTALLATION_CANCELLABLE.store(true, Ordering::Release);
}

fn stop_tracking_for_cleanup() {
    INSTALLATION_CANCELLABLE.store(false, Ordering::Release);
    ACTIVE_INSTALLATION_PROCESS_GROUP.store(0, Ordering::Release);
}

fn finish_cancellable_installation() {
    stop_tracking_for_cleanup();
    INSTALLATION_CANCEL_REQUESTED.store(false, Ordering::Release);
}

fn run(command: &str, args: &[&str]) -> Result<(), String> {
    let mut child = Command::new("pkexec")
        .process_group(0)
        .arg(privileged_program(command))
        .args(args)
        .spawn()
        .map_err(|_| generic_command_error(command))?;
    let status =
        wait_for_privileged_child(&mut child).map_err(|_| generic_command_error(command))?;
    if INSTALLATION_CANCEL_REQUESTED.load(Ordering::Acquire) {
        Err(CANCELLED_MESSAGE.to_string())
    } else if status.success() {
        Ok(())
    } else {
        Err(generic_command_error(command))
    }
}

fn run_cleanup(command: &str, args: &[&str]) -> Result<(), String> {
    let status = Command::new("pkexec")
        .arg(privileged_program(command))
        .args(args)
        .status()
        .map_err(|_| generic_command_error(command))?;
    if status.success() {
        Ok(())
    } else {
        Err(generic_command_error(command))
    }
}

fn command_output(command: &str, args: &[&str]) -> std::io::Result<std::process::Output> {
    // Read only helper commands still get their own process group so they can
    // be stopped immediately during cancellation.
    let child = Command::new(command)
        .process_group(0)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let process_group_id = child.id();
    track_privileged_child(&child);
    let result = child.wait_with_output();
    clear_tracked_child(process_group_id);
    result
}

fn generic_command_error(command: &str) -> String {
    if matches!(
        command,
        "wipefs" | "parted" | "mkfs.fat" | "mkswap" | "mkfs.btrfs" | "mount" | "swapon"
    ) {
        format!("Failed to format drive - {command} failed.")
    } else {
        format!("Failed to configure system - {command} failed.")
    }
}

fn run_bootloader(command: &str, args: &[&str]) -> Result<(), String> {
    let mut child = Command::new("pkexec")
        .process_group(0)
        .arg(privileged_program(command))
        .args(args)
        .spawn()
        .map_err(|_| "Failed to configure bootloader.".to_string())?;
    let status = wait_for_privileged_child(&mut child)
        .map_err(|_| "Failed to configure bootloader.".to_string())?;
    if status.success() {
        Ok(())
    } else {
        Err("Failed to configure bootloader.".to_string())
    }
}

fn format_drive(command: &str, args: &[&str]) -> Result<(), String> {
    run(command, args).map_err(|error| {
        if error == CANCELLED_MESSAGE {
            error
        } else {
            format!("Failed to format drive - {command} failed.")
        }
    })
}

fn field(line: &str, key: &str) -> String {
    let marker = format!("{key}=\"");
    let mut search_from = 0;
    while let Some(relative_start) = line[search_from..].find(&marker) {
        let start = search_from + relative_start;
        let exact_key = start == 0 || line.as_bytes()[start - 1].is_ascii_whitespace();
        let value_start = start + marker.len();
        if exact_key && let Some(end) = line[value_start..].find('"') {
            return decode_lsblk_value(&line[value_start..value_start + end]);
        }
        search_from = value_start;
    }
    String::new()
}

fn decode_lsblk_value(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'\\'
            && index + 3 < bytes.len()
            && bytes[index + 1] == b'x'
            && let Ok(hex) = std::str::from_utf8(&bytes[index + 2..index + 4])
            && let Ok(byte) = u8::from_str_radix(hex, 16)
        {
            decoded.push(byte);
            index += 4;
        } else {
            decoded.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8_lossy(&decoded).into_owned()
}

fn validate_target(target_disk: &str) -> Result<(), String> {
    let output = command_output(
        "lsblk",
        &[
            "-d",
            "-b",
            "-P",
            "--output",
            "NAME,SIZE,TYPE",
            "--noheadings",
        ],
    )
    .map_err(|_| {
        "Failed to prepare the drive: lsblk failed. The drive was not formatted.".to_string()
    })?;

    for line in String::from_utf8_lossy(&output.stdout).lines() {
        let device = field(line, "NAME");
        let size_bytes = field(line, "SIZE").parse::<u64>().unwrap_or(0);
        if field(line, "TYPE") == "disk" && format!("/dev/{device}") == target_disk {
            if size_bytes < MIN_STORAGE_GIB * 1024 * 1024 * 1024 {
                return Err(format!(
                    "The selected drive must have at least {MIN_STORAGE_GIB} GiB of usable space."
                ));
            }
            return Ok(());
        }
    }
    Err("The selected installation drive is no longer available.".to_string())
}

fn disable_swap_on_target(target_disk: &str) -> Result<(), String> {
    let output = command_output("swapon", &["--noheadings", "--raw", "--show=NAME"])
        .map_err(|_| "swapon failed".to_string())?;

    for line in String::from_utf8_lossy(&output.stdout).lines() {
        let swap_device = line.trim();
        if !swap_device.is_empty() && swap_device.starts_with(target_disk) {
            run("swapoff", &[swap_device])
                .map_err(|_| format!("swapoff failed for {swap_device}"))?;
        }
    }
    Ok(())
}

fn force_unmount_target(target_disk: &str) -> Result<(), String> {
    let output = command_output("lsblk", &["-nr", "-o", "MOUNTPOINT", target_disk])
        .map_err(|_| "lsblk failed".to_string())?;

    let mut mountpoints = String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(|line| line.trim().to_string())
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>();
    mountpoints.sort_by_key(|right| std::cmp::Reverse(right.len()));

    for mountpoint in mountpoints {
        let _ = run("fuser", &["-km", &mountpoint]);
        run("umount", &["-lf", &mountpoint])
            .map_err(|_| format!("unmount failed for {mountpoint}"))?;
    }
    Ok(())
}

fn reinforce_root_labels(target_disk: &str, root_part: &str) {
    // Some tools cache partition metadata briefly after formatting. Confirm
    // both labels once udev has settled and repair either one if necessary.
    let _ = run("udevadm", &["settle"]);
    let Ok(output) = command_output("lsblk", &["-dn", "-P", "-o", "PARTLABEL,LABEL", root_part])
    else {
        return;
    };
    let line = String::from_utf8_lossy(&output.stdout);
    if field(&line, "PARTLABEL") != "Fluff Linux" {
        let _ = run(
            "parted",
            &["--script", target_disk, "name", "3", "\"Fluff Linux\""],
        );
    }
    if field(&line, "LABEL") != "Fluff Linux" {
        let _ = run("btrfs", &["filesystem", "label", root_part, "Fluff Linux"]);
    }
}

fn transaction_counter(line: &str) -> Option<(usize, usize)> {
    let mut result = None;
    let mut search_from = 0;
    while let Some(relative_close) = line[search_from..].find(')') {
        let close = search_from + relative_close;
        if let Some(relative_open) = line[search_from..close].rfind('(') {
            let open = search_from + relative_open;
            if let Some((completed, total)) = line[open + 1..close].split_once('/')
                && let (Ok(completed), Ok(total)) = (
                    completed.trim().parse::<usize>(),
                    total.trim().parse::<usize>(),
                )
                && total > 0
                && completed <= total
            {
                result = Some((completed, total));
            }
        }
        search_from = close + 1;
    }
    result
}

fn forward_progress_stream(mut stream: impl Read, sender: std::sync::mpsc::Sender<String>) {
    // Pacman redraws progress with carriage returns. Treat both carriage
    // returns and newlines as complete updates for the graphical interface.
    let mut pending = Vec::new();
    let mut buffer = [0_u8; 4096];
    loop {
        let Ok(read) = stream.read(&mut buffer) else {
            break;
        };
        if read == 0 {
            break;
        }
        for byte in &buffer[..read] {
            if *byte == b'\n' || *byte == b'\r' {
                if !pending.is_empty() {
                    let event = String::from_utf8_lossy(&pending).into_owned();
                    let _ = sender.send(event);
                    pending.clear();
                }
            } else {
                pending.push(*byte);
            }
        }
    }
    if !pending.is_empty() {
        let _ = sender.send(String::from_utf8_lossy(&pending).into_owned());
    }
}

fn transaction_error(output: &std::process::Output) -> String {
    let message = String::from_utf8_lossy(&output.stderr)
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .last()
        .unwrap_or("pacman could not prepare the installation")
        .to_string();
    format!("Failed to prepare the system installation - {message}")
}

fn resolve_installation_total(
    packages: &[&str],
    cancel_requested: &AtomicBool,
) -> Result<Option<usize>, String> {
    run(
        "mkdir",
        &[
            "-p",
            "/mnt/var/lib/pacman/local",
            "/mnt/var/cache/pacman/pkg",
        ],
    )
    .map_err(|_| "Failed to prepare the system installation - mkdir failed.".to_string())?;

    let mut child = Command::new("pkexec")
        .process_group(0)
        .arg(privileged_program("pacman"))
        .args([
            "--root",
            "/mnt",
            "--config",
            "/etc/pacman.d/fluffinstall.conf",
            "--gpgdir",
            "/etc/pacman.d/gnupg",
            "-Syp",
            "--noconfirm",
            "--print-format",
            "%l",
        ])
        .args(packages)
        .env("LC_ALL", "C")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("Failed to prepare the system installation - {error}"))?;
    track_privileged_child(&child);
    let process_group_id = child.id();

    let mut stdout_reader = child.stdout.take().map(|mut stream| {
        thread::spawn(move || {
            let mut output = Vec::new();
            let _ = stream.read_to_end(&mut output);
            output
        })
    });
    let mut stderr_reader = child.stderr.take().map(|mut stream| {
        thread::spawn(move || {
            let mut output = Vec::new();
            let _ = stream.read_to_end(&mut output);
            output
        })
    });

    let status = loop {
        if cancel_requested.load(Ordering::Relaxed) {
            if force_kill_and_reap(&mut child) {
                if let Some(reader) = stdout_reader.take() {
                    let _ = reader.join();
                }
                if let Some(reader) = stderr_reader.take() {
                    let _ = reader.join();
                }
            }
            clear_tracked_child(process_group_id);
            return Err(CANCELLED_MESSAGE.to_string());
        }

        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => thread::sleep(Duration::from_millis(100)),
            Err(error) => {
                force_kill_and_reap(&mut child);
                clear_tracked_child(process_group_id);
                return Err(format!(
                    "Failed to prepare the system installation - {error}"
                ));
            }
        }
    };
    clear_tracked_child(process_group_id);

    let output = std::process::Output {
        status,
        stdout: stdout_reader
            .and_then(|reader| reader.join().ok())
            .unwrap_or_default(),
        stderr: stderr_reader
            .and_then(|reader| reader.join().ok())
            .unwrap_or_default(),
    };

    if !output.status.success() {
        return Err(transaction_error(&output));
    }

    let targets = String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .filter(|line| line.contains(".pkg.tar.") && !line.ends_with(".sig"))
        .map(str::to_string)
        .collect::<std::collections::HashSet<_>>();

    if targets.is_empty() {
        Ok(None)
    } else {
        Ok(Some(targets.len()))
    }
}

fn installed_package_count(log_path: &str) -> usize {
    fs::read_to_string(log_path)
        .map(|log| {
            log.lines()
                .filter(|line| line.contains("[ALPM] installed "))
                .count()
        })
        .unwrap_or(0)
}

fn is_installation_counter(line: &str) -> bool {
    let line = line.to_ascii_lowercase();
    ["installing", "upgrading", "downgrading", "reinstalling"]
        .iter()
        .any(|operation| line.contains(operation))
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn verification_stage(
    line: &str,
) -> Option<(&'static str, &'static str, i32, i32, Option<(usize, usize)>)> {
    let line = line.to_ascii_lowercase();
    [
        (
            "checking keys in keyring",
            (
                "Loading system files...",
                "system files loaded",
                24,
                26,
                None,
            ),
        ),
        (
            "checking package integrity",
            (
                "Verifying system files...",
                "system files verified",
                26,
                30,
                Some((1, 2)),
            ),
        ),
        (
            "loading package files",
            (
                "Verifying system files...",
                "system files verified",
                30,
                35,
                Some((2, 2)),
            ),
        ),
    ]
    .into_iter()
    .filter_map(|(phrase, stage)| line.rfind(phrase).map(|position| (position, stage)))
    .max_by_key(|(position, _)| *position)
    .map(|(_, stage)| stage)
}

fn force_kill_process_group(process_group_id: u32) {
    let negative_process_group_id = format!("-{process_group_id}");
    let _ = Command::new("pkexec")
        .arg(privileged_program("kill"))
        .args(["-KILL", "--", &negative_process_group_id])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();

    // Keep a recursive fallback for an older child that was not started in a
    // dedicated process group. New installation commands use the group kill.
    const KILL_TREE_SCRIPT: &str = r#"
kill_tree() {
    process_id="$1"
    children="$(cat "/proc/$process_id/task/$process_id/children" 2>/dev/null)" || children=""
    for child_id in $children; do
        kill_tree "$child_id"
    done
    kill -KILL "$process_id" 2>/dev/null || true
}
kill_tree "$1"
"#;
    let root_pid = process_group_id.to_string();

    let _ = Command::new("pkexec")
        .arg(privileged_program("sh"))
        .args(["-c", KILL_TREE_SCRIPT, "fluffinstall-cancel", &root_pid])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

fn force_kill_and_reap(child: &mut std::process::Child) -> bool {
    for _ in 0..3 {
        force_kill_process_group(child.id());
        let _ = child.kill();

        for _ in 0..10 {
            if child.try_wait().is_ok_and(|status| status.is_some()) {
                return true;
            }
            thread::sleep(Duration::from_millis(50));
        }
    }
    false
}

fn install_packages(
    callback: &mut impl FnMut(Progress),
    cancel_requested: &AtomicBool,
) -> Result<(), String> {
    let packages = PACKAGE_LIST.split_whitespace().collect::<Vec<_>>();
    let mut total = resolve_installation_total(&packages, cancel_requested)?.unwrap_or(0);
    let initial_detail = if total > 0 {
        format!("0 / {total} system files loaded")
    } else {
        "Preparing system installation...".to_string()
    };
    report_with_detail(
        callback,
        24,
        0,
        total as i32,
        "Loading system files...",
        &initial_detail,
    );
    // Pacman output differs slightly between releases. The visible counter
    // uses both terminal output and package logs, then keeps the largest value.
    let target_log_baseline = installed_package_count("/mnt/var/log/pacman.log");
    let host_log_baseline = installed_package_count("/var/log/pacman.log");

    let mut pacstrap_arguments = vec![
        privileged_program("pacstrap"),
        "-C".to_string(),
        "/etc/pacman.d/fluffinstall.conf".to_string(),
        "-K".to_string(),
        "/mnt".to_string(),
    ];
    pacstrap_arguments.extend(packages.iter().map(|package| package.to_string()));
    let pacstrap_command = pacstrap_arguments
        .iter()
        .map(|argument| shell_quote(argument))
        .collect::<Vec<_>>()
        .join(" ");

    let mut child = Command::new("pkexec")
        .process_group(0)
        .arg(privileged_program("script"))
        .args([
            "--quiet",
            "--return",
            "--flush",
            "--command",
            &pacstrap_command,
            "/dev/null",
        ])
        .env("LC_ALL", "C")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("Failed to initiate system installation - {error}"))?;
    track_privileged_child(&child);
    let process_group_id = child.id();

    let (sender, receiver) = std::sync::mpsc::channel::<String>();
    if let Some(stdout) = child.stdout.take() {
        let stdout_sender = sender.clone();
        thread::spawn(move || forward_progress_stream(stdout, stdout_sender));
    }
    if let Some(stderr) = child.stderr.take() {
        let stderr_sender = sender.clone();
        thread::spawn(move || forward_progress_stream(stderr, stderr_sender));
    }
    drop(sender);

    let mut output_completed = 0;
    let mut visible_completed = 0;
    let mut visible_overall = 24;
    let mut visible_verification_status = "Loading system files...";
    let mut pacstrap_failure_detail = None;
    let status = loop {
        if cancel_requested.load(Ordering::Relaxed) {
            force_kill_and_reap(&mut child);
            clear_tracked_child(process_group_id);
            return Err(CANCELLED_MESSAGE.to_string());
        }

        match receiver.recv_timeout(Duration::from_millis(100)) {
            Ok(line) => {
                let trimmed_line = line.trim();
                if !trimmed_line.is_empty()
                    && (trimmed_line.to_ascii_lowercase().contains("error:")
                        || trimmed_line.to_ascii_lowercase().contains("failed"))
                {
                    pacstrap_failure_detail = Some(trimmed_line.to_string());
                }
                if let Some((status, counter_label, phase_start, phase_end, verification_pass)) =
                    verification_stage(&line)
                {
                    if let Some((completed, reported_total)) = transaction_counter(&line) {
                        let verification_overall = phase_start
                            + (completed * (phase_end - phase_start) as usize / reported_total)
                                as i32;
                        visible_overall = visible_overall.max(verification_overall);
                        visible_verification_status = status;
                        let detail = match verification_pass {
                            Some((pass, pass_total)) => format!(
                                "Pass {pass} of {pass_total}\n{completed} / {reported_total} {counter_label}"
                            ),
                            None => format!("{completed} / {reported_total} {counter_label}"),
                        };
                        report_with_detail(
                            callback,
                            visible_overall,
                            completed as i32,
                            reported_total as i32,
                            status,
                            &detail,
                        );
                    } else if status != visible_verification_status {
                        visible_verification_status = status;
                        let detail = if total > 0 {
                            match verification_pass {
                                Some((pass, pass_total)) => format!(
                                    "Pass {pass} of {pass_total}\n0 / {total} {counter_label}"
                                ),
                                None => format!("0 / {total} {counter_label}"),
                            }
                        } else {
                            "Preparing system installation...".to_string()
                        };
                        report_with_detail(
                            callback,
                            visible_overall,
                            0,
                            total as i32,
                            status,
                            &detail,
                        );
                    }
                } else if let Some((completed, reported_total)) = transaction_counter(&line)
                    && is_installation_counter(&line)
                {
                    output_completed = output_completed.max(completed);
                    if reported_total > 0 {
                        total = reported_total;
                    }
                }
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                thread::sleep(Duration::from_millis(100));
            }
        }

        let target_log_completed =
            installed_package_count("/mnt/var/log/pacman.log").saturating_sub(target_log_baseline);
        let host_log_completed =
            installed_package_count("/var/log/pacman.log").saturating_sub(host_log_baseline);
        let completed = output_completed
            .max(target_log_completed)
            .max(host_log_completed);
        if total > 0 && completed != visible_completed {
            let completed = completed.min(total);
            visible_completed = completed;
            visible_overall = visible_overall.max(35 + (completed * 43 / total) as i32);
            report(
                callback,
                visible_overall,
                completed as i32,
                total as i32,
                "Installing system files...",
            );
        }

        if let Some(status) = child
            .try_wait()
            .map_err(|error| format!("Failed to install system files, pacstrap failed - {error}"))?
        {
            clear_tracked_child(process_group_id);
            break status;
        }
    };

    if !status.success() {
        return Err(match pacstrap_failure_detail {
            Some(detail) => format!("Failed to install system files, pacstrap failed - {detail}"),
            None => "Failed to install system files, pacstrap failed.".to_string(),
        });
    }
    report(
        callback,
        78,
        total as i32,
        total as i32,
        "Installing system files...",
    );
    Ok(())
}

fn set_password(username: &str, password: &str) -> Result<(), String> {
    let mut child = Command::new("pkexec")
        .process_group(0)
        .arg(privileged_program("arch-chroot"))
        .arg("/mnt")
        .arg("chpasswd")
        .stdin(Stdio::piped())
        .spawn()
        .map_err(|_| "Failed to configure system - chpasswd failed.".to_string())?;
    child
        .stdin
        .as_mut()
        .ok_or_else(|| "Failed to configure system - chpasswd failed.".to_string())?
        .write_all(format!("{username}:{password}\n").as_bytes())
        .map_err(|_| "Failed to configure system - chpasswd failed.".to_string())?;
    if wait_for_privileged_child(&mut child)
        .map_err(|_| "Failed to configure system - chpasswd failed.".to_string())?
        .success()
    {
        Ok(())
    } else {
        Err("Failed to configure system - chpasswd failed.".to_string())
    }
}

const NON_LOCALE_CONFIGURATION_OPERATIONS: usize = 60;

struct ConfigurationProgress {
    completed: usize,
    total: usize,
}

impl ConfigurationProgress {
    fn report(&self, callback: &mut impl FnMut(Progress)) {
        let (overall, configuration_percent) = if self.total > 0 {
            let completed = self.completed.min(self.total);
            (
                78 + (completed * 20 / self.total) as i32,
                completed * 100 / self.total,
            )
        } else {
            (78, 0)
        };
        report_with_detail(
            callback,
            overall,
            self.completed as i32,
            self.total as i32,
            "Configuring system...",
            &format!("{configuration_percent}% complete"),
        );
    }

    fn complete(&mut self, callback: &mut impl FnMut(Progress)) {
        self.completed = self.completed.saturating_add(1);
        self.report(callback);
    }
}

fn utf8_locale_count() -> usize {
    fs::read_to_string("/mnt/etc/locale.gen")
        .map(|contents| {
            contents
                .lines()
                .filter(|line| {
                    let line = line.trim();
                    if line.is_empty() {
                        return false;
                    }
                    let line = line.strip_prefix('#').unwrap_or(line).trim_start();
                    let mut fields = line.split_whitespace();
                    fields.next().is_some() && fields.next() == Some("UTF-8")
                })
                .count()
        })
        .unwrap_or(0)
}

fn enable_all_utf8_locales(
    callback: &mut impl FnMut(Progress),
    progress: &mut ConfigurationProgress,
) -> Result<(), String> {
    // FluffSetup may choose any language or regional format on first boot.
    // Generate every UTF8 locale now so later changes do not need downloads.
    run(
        "arch-chroot",
        &[
            "/mnt",
            "sh",
            "-c",
            r#"sed -E -i 's/^#[[:space:]]*([[:alnum:]_@.-]+[[:space:]]+UTF-8[[:space:]]*)$/\1/' /etc/locale.gen"#,
        ],
    )
    .map_err(|_| "Failed to configure system - locale-gen failed.".to_string())?;
    progress.complete(callback);

    let mut child = Command::new("pkexec")
        .process_group(0)
        .arg(privileged_program("arch-chroot"))
        .args(["/mnt", "locale-gen"])
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|_| "Failed to configure system - locale-gen failed.".to_string())?;
    track_privileged_child(&child);

    if let Some(stdout) = child.stdout.take() {
        for line in BufReader::new(stdout).lines() {
            let Ok(line) = line else {
                break;
            };
            if line.trim_end().ends_with("... done") {
                progress.complete(callback);
            }
        }
    }

    if wait_for_privileged_child(&mut child)
        .map_err(|_| "Failed to configure system - locale-gen failed.".to_string())?
        .success()
    {
        Ok(())
    } else {
        Err("Failed to configure system - locale-gen failed.".to_string())
    }
}

fn sync_and_unmount_target() {
    // A failed installation still needs a best effort cleanup before the
    // error is shown. Repeated normal unmounts give processes time to exit.
    let _ = run_cleanup("pkill", &["-KILL", "gpg-agent"]);
    let _ = run_cleanup("sync", &[]);
    for _ in 0..10 {
        if Command::new("pkexec")
            .arg(privileged_program("umount"))
            .args(["-R", "/mnt"])
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|status| status.success())
        {
            return;
        }
        thread::sleep(Duration::from_secs(1));
    }
}

fn finalize_target() -> Result<(), String> {
    let _ = run("pkill", &["-KILL", "gpg-agent"]);
    run("sync", &[])?;
    for _ in 0..10 {
        match run("umount", &["-R", "/mnt"]) {
            Ok(()) => return Ok(()),
            Err(error) if error == CANCELLED_MESSAGE => return Err(error),
            Err(_) => thread::sleep(Duration::from_secs(1)),
        }
    }
    Ok(())
}

fn cancel_and_unmount_target() {
    // Cancellation favors speed. Active work is already killed, so a single
    // forced recursive unmount is safer than leaving the target attached.
    let _ = run_cleanup("pkill", &["-KILL", "gpg-agent"]);
    let _ = Command::new("pkexec")
        .arg(privileged_program("umount"))
        .args(["-Rlf", "/mnt"])
        .stderr(Stdio::null())
        .status();
}

fn cancel_with_cleanup_if_requested(
    cancel_requested: &AtomicBool,
    swap_part: &str,
) -> Result<(), String> {
    if cancel_requested.load(Ordering::Relaxed) {
        stop_tracking_for_cleanup();
        let _ = run_cleanup("swapoff", &[swap_part]);
        cancel_and_unmount_target();
        INSTALLATION_CANCEL_REQUESTED.store(false, Ordering::Release);
        Err(CANCELLED_MESSAGE.to_string())
    } else {
        Ok(())
    }
}

pub fn install(
    target_disk: &str,
    hostname: &str,
    cancel_requested: &AtomicBool,
    mut callback: impl FnMut(Progress),
) -> Result<(), String> {
    let part_suffix = if target_disk.contains("nvme") || target_disk.contains("mmcblk") {
        "p"
    } else {
        ""
    };
    let boot_part = format!("{target_disk}{part_suffix}1");
    let swap_part = format!("{target_disk}{part_suffix}2");
    let root_part = format!("{target_disk}{part_suffix}3");
    begin_cancellable_installation(cancel_requested.load(Ordering::Acquire));

    // Before formatting, failures can return directly because the selected
    // drive has not been changed or mounted by this installer.
    macro_rules! preinstallation_operation {
        ($operation:expr) => {{
            let operation_result = $operation;
            if cancel_requested.load(Ordering::Relaxed)
                || operation_result
                    .as_ref()
                    .is_err_and(|error| error == CANCELLED_MESSAGE)
            {
                finish_cancellable_installation();
                return Err(CANCELLED_MESSAGE.to_string());
            }
            match operation_result {
                Ok(value) => value,
                Err(error) => {
                    finish_cancellable_installation();
                    return Err(error);
                }
            }
        }};
    }

    // Once drive work begins, every exit path disables swap and unmounts the
    // target. Cancellation uses forced cleanup while other failures sync first.
    macro_rules! installation_operation {
        ($operation:expr) => {{
            let operation_result = $operation;
            if cancel_requested.load(Ordering::Relaxed)
                || operation_result
                    .as_ref()
                    .is_err_and(|error| error == CANCELLED_MESSAGE)
            {
                stop_tracking_for_cleanup();
                let _ = run_cleanup("swapoff", &[&swap_part]);
                cancel_and_unmount_target();
                INSTALLATION_CANCEL_REQUESTED.store(false, Ordering::Release);
                return Err(CANCELLED_MESSAGE.to_string());
            }
            match operation_result {
                Ok(value) => value,
                Err(error) => {
                    stop_tracking_for_cleanup();
                    let _ = run_cleanup("swapoff", &[&swap_part]);
                    sync_and_unmount_target();
                    INSTALLATION_CANCEL_REQUESTED.store(false, Ordering::Release);
                    return Err(error);
                }
            }
        }};
    }

    preinstallation_operation!(validate_target(target_disk));
    preinstallation_operation!(validate_fluffsetup_session_files());
    preinstallation_operation!(check_cancelled(cancel_requested));

    report(&mut callback, 2, 0, 0, "Preparing...");
    installation_operation!(disable_swap_on_target(target_disk).map_err(|error| {
        if error == CANCELLED_MESSAGE {
            error
        } else {
            format!("Failed to prepare the drive: {error}. The drive was not formatted.")
        }
    }));
    installation_operation!(force_unmount_target(target_disk).map_err(|error| {
        if error == CANCELLED_MESSAGE {
            error
        } else {
            format!("Failed to prepare the drive: {error}. The drive was not formatted.")
        }
    }));

    // Formatting and partitioning copied in the same order from FluffInstall 0.9.
    installation_operation!(format_drive("wipefs", &["--all", target_disk]));
    installation_operation!(format_drive(
        "parted",
        &["--script", target_disk, "mklabel", "gpt"]
    ));
    installation_operation!(format_drive(
        "parted",
        &[
            "--script",
            target_disk,
            "mkpart",
            "primary",
            "fat32",
            "1MiB",
            "1GiB",
        ],
    ));
    installation_operation!(format_drive(
        "parted",
        &["--script", target_disk, "set", "1", "esp", "on"],
    ));
    installation_operation!(format_drive(
        "parted",
        &["--script", target_disk, "name", "1", "EFI"]
    ));
    installation_operation!(format_drive(
        "parted",
        &[
            "--script",
            target_disk,
            "mkpart",
            "primary",
            "linux-swap",
            "1GiB",
            "5GiB",
        ],
    ));
    installation_operation!(format_drive(
        "parted",
        &["--script", target_disk, "name", "2", "SWAP"]
    ));
    installation_operation!(format_drive(
        "parted",
        &[
            "--script",
            target_disk,
            "mkpart",
            "primary",
            "btrfs",
            "5GiB",
            "100%",
        ],
    ));
    installation_operation!(format_drive(
        "parted",
        &["--script", target_disk, "name", "3", "\"Fluff Linux\""],
    ));

    installation_operation!(format_drive("wipefs", &["-a", &boot_part]));
    installation_operation!(format_drive("wipefs", &["-a", &swap_part]));
    installation_operation!(format_drive("wipefs", &["-a", &root_part]));
    report(&mut callback, 14, 0, 0, "Formatting drive...");
    installation_operation!(format_drive("mkfs.fat", &["-F32", "-n", "EFI", &boot_part]));
    installation_operation!(format_drive("mkswap", &["-L", "SWAP", &swap_part]));
    installation_operation!(format_drive(
        "mkfs.btrfs",
        &["-f", "-L", "Fluff Linux", &root_part]
    ));
    reinforce_root_labels(target_disk, &root_part);
    installation_operation!(format_drive(
        "mount",
        &["-o", "compress=zstd,noatime", &root_part, "/mnt"],
    ));
    installation_operation!(format_drive("mount", &["--mkdir", &boot_part, "/mnt/boot"]));
    installation_operation!(format_drive("swapon", &[&swap_part]));

    report(&mut callback, 24, 0, 0, "Installing system files...");
    installation_operation!(install_packages(&mut callback, cancel_requested));
    cancel_with_cleanup_if_requested(cancel_requested, &swap_part)?;

    let mut configuration_progress = ConfigurationProgress {
        completed: 0,
        total: NON_LOCALE_CONFIGURATION_OPERATIONS + utf8_locale_count(),
    };
    configuration_progress.report(&mut callback);

    macro_rules! config_operation {
        ($operation:expr) => {{
            let operation_result = $operation;
            installation_operation!(operation_result);
            configuration_progress.complete(&mut callback);
        }};
    }

    config_operation!(run("cp", &["/etc/motd", "/mnt/etc/"]));
    config_operation!(run("cp", &["/etc/issue", "/mnt/etc/"]));
    config_operation!(run("cp", &["-r", "/etc/skel", "/mnt/etc/"]));
    config_operation!(run("cp", &["/etc/nanorc", "/mnt/etc/"]));
    config_operation!(run("cp", &["/etc/pacman.conf", "/mnt/etc/"]));
    config_operation!(run(
        "cp",
        &["/etc/pacman.d/mirrorlist", "/mnt/etc/pacman.d/mirrorlist"],
    ));
    config_operation!(run("cp", &["/etc/locale.conf", "/mnt/etc/"]));
    installation_operation!(enable_all_utf8_locales(
        &mut callback,
        &mut configuration_progress,
    ));

    config_operation!(run(
        "cp",
        &[
            "/etc/fonts/conf.d/99-emoji-fallback.conf",
            "/mnt/etc/fonts/conf.d/",
        ],
    ));
    config_operation!(run("mkdir", &["-p", "/mnt/etc/plasmalogin.conf.d"]));
    config_operation!(run(
        "cp",
        &[
            "/usr/lib/fluffinstall/plasmalogin-etc/flufflinux.conf",
            "/mnt/etc/plasmalogin.conf.d",
        ],
    ));
    config_operation!(run(
        "cp",
        &[
            "/usr/lib/fluffinstall/plasmalogin-etc/plasmalogin.conf",
            "/mnt/etc/",
        ],
    ));
    config_operation!(run(
        "install",
        &[
            "-Dm644",
            FLUFFSETUP_DESKTOP_SOURCE,
            "/mnt/usr/share/wayland-sessions/fluffsetup.desktop",
        ],
    ));
    config_operation!(run(
        "install",
        &[
            "-Dm755",
            FLUFFSETUP_SESSION_SOURCE,
            "/mnt/usr/lib/fluffsetup/fluffsetup-session",
        ],
    ));
    config_operation!(run(
        "install",
        &[
            "-Dm755",
            FLUFFSETUP_BINARY_SOURCE,
            "/mnt/usr/bin/fluffsetup",
        ],
    ));
    config_operation!(run(
        "cp",
        &["-r", "/usr/lib/fluffinstall/plasmalogin", "/mnt/var/lib/"],
    ));
    config_operation!(run(
        "arch-chroot",
        &[
            "/mnt",
            "chown",
            "-R",
            "plasmalogin:plasmalogin",
            "/var/lib/plasmalogin",
        ],
    ));
    config_operation!(run(
        "ln",
        &["-sf", "/usr/share/zoneinfo/UTC", "/mnt/etc/localtime"],
    ));
    config_operation!(run(
        "cp",
        &[
            "/usr/lib/firefox/distribution/policies.json",
            "/mnt/usr/lib/firefox/distribution/",
        ],
    ));

    config_operation!(run("cp", &["-r", "/etc/getwine", "/mnt/etc/"]));
    config_operation!(run("cp", &["/usr/bin/getwine", "/mnt/usr/bin"]));
    config_operation!(run(
        "arch-chroot",
        &["/mnt", "chmod", "+x", "/usr/bin/getwine"]
    ));
    config_operation!(run(
        "cp",
        &[
            "/etc/getwine/getwine.desktop",
            "/mnt/usr/share/applications",
        ],
    ));
    config_operation!(run(
        "arch-chroot",
        &[
            "/mnt",
            "chmod",
            "+x",
            "/usr/share/applications/getwine.desktop",
        ],
    ));

    config_operation!(run("sh", &["-c", "genfstab -U /mnt >> /mnt/etc/fstab"]));
    config_operation!(run(
        "sh",
        &["-c", "arch-chroot /mnt fc-cache -fv 2>/dev/null"]
    ));

    cancel_with_cleanup_if_requested(cancel_requested, &swap_part)?;
    config_operation!(run_bootloader(
        "arch-chroot",
        &[
            "/mnt",
            "grub-install",
            "--target=x86_64-efi",
            "--efi-directory=/boot",
            "--removable",
            "--boot-directory=/boot",
        ],
    ));
    config_operation!(run_bootloader(
        "cp",
        &["/etc/default/grub", "/mnt/etc/default/grub"]
    ));
    config_operation!(run_bootloader(
        "cp",
        &["/etc/grub.d/10_linux", "/mnt/etc/grub.d/10_linux"]
    ));
    config_operation!(run_bootloader(
        "arch-chroot",
        &["/mnt", "grub-mkconfig", "-o", "/boot/grub/grub.cfg"],
    ));

    cancel_with_cleanup_if_requested(cancel_requested, &swap_part)?;
    config_operation!(run(
        "sh",
        &["-c", &format!("echo \"{hostname}\" > /mnt/etc/hostname")],
    ));
    let username = "fluffsetup";
    config_operation!(run(
        "arch-chroot",
        &["/mnt", "usermod", "-s", "/bin/zsh", "root"],
    ));
    config_operation!(run(
        "arch-chroot",
        &[
            "/mnt",
            "useradd",
            "-m",
            "-G",
            "uucp,wheel,kvm,libvirt",
            "-s",
            "/bin/zsh",
            username,
        ],
    ));
    config_operation!(run(
        "arch-chroot",
        &[
            "/mnt",
            "sh",
            "-c",
            &format!(
                "sed -i 's|^HomeUrl=/home/|HomeUrl=/home/{username}/|' /home/{username}/.config/dolphinrc && sed -i '/^HomeUrl=/a RememberOpenedTabs=false' /home/{username}/.config/dolphinrc"
            ),
        ],
    ));
    config_operation!(run(
        "arch-chroot",
        &[
            "/mnt",
            "sed",
            "-i",
            "s/^# %wheel ALL=(ALL:ALL) ALL/%wheel ALL=(ALL:ALL) ALL/",
            "/etc/sudoers",
        ],
    ));
    config_operation!(run(
        "arch-chroot",
        &[
            "/mnt",
            "sh",
            "-c",
            "echo 'Defaults env_keep += \"VISUAL EDITOR\"' >> /etc/sudoers",
        ],
    ));
    config_operation!(run(
        "arch-chroot",
        &[
            "/mnt",
            "sh",
            "-c",
            &format!(
                "install -d -m 0750 /etc/sudoers.d && printf '%s\\n' 'fluffsetup ALL=(ALL:ALL) NOPASSWD: ALL' > {FLUFFSETUP_SUDOERS_PATH} && chmod 0440 {FLUFFSETUP_SUDOERS_PATH} && visudo -cf /etc/sudoers"
            ),
        ],
    ));
    config_operation!(set_password(username, "fluff"));
    config_operation!(run(
        "chown",
        &[
            "root:root",
            &format!("/mnt/home/{username}/Desktop/trash:⁄.desktop"),
        ],
    ));

    config_operation!(run(
        "arch-chroot",
        &["/mnt", "systemctl", "enable", "plasmalogin.service"],
    ));
    config_operation!(run(
        "arch-chroot",
        &["/mnt", "systemctl", "enable", "NetworkManager"],
    ));
    config_operation!(run(
        "arch-chroot",
        &[
            "/mnt",
            "ln",
            "-sf",
            "/run/NetworkManager/resolv.conf",
            "/etc/resolv.conf",
        ],
    ));
    config_operation!(run(
        "arch-chroot",
        &["/mnt", "systemctl", "enable", "bluetooth"]
    ));
    config_operation!(run("arch-chroot", &["/mnt", "systemctl", "enable", "tlp"]));
    config_operation!(run(
        "arch-chroot",
        &["/mnt", "systemctl", "enable", "fstrim.timer"],
    ));
    config_operation!(run("arch-chroot", &["/mnt", "systemctl", "enable", "cups"]));
    config_operation!(run(
        "arch-chroot",
        &["/mnt", "systemctl", "enable", "avahi-daemon"],
    ));
    config_operation!(run(
        "arch-chroot",
        &["/mnt", "systemctl", "enable", "thermald.service"],
    ));
    config_operation!(run(
        "arch-chroot",
        &["/mnt", "systemctl", "enable", "systemd-timesyncd"],
    ));
    config_operation!(run(
        "arch-chroot",
        &["/mnt", "systemctl", "enable", "cronie"]
    ));

    config_operation!(run(
        "arch-chroot",
        &["/mnt", "systemctl", "enable", "libvirtd"]
    ));
    config_operation!(run(
        "arch-chroot",
        &["/mnt", "systemctl", "enable", "virtqemud"]
    ));
    config_operation!(run(
        "arch-chroot",
        &["/mnt", "systemctl", "enable", "virtstoraged"],
    ));
    config_operation!(run(
        "arch-chroot",
        &["/mnt", "systemctl", "enable", "virtnetworkd"],
    ));
    config_operation!(run(
        "arch-chroot",
        &["/mnt", "systemctl", "enable", "virtlogd"]
    ));
    config_operation!(run(
        "arch-chroot",
        &["/mnt", "systemctl", "enable", "virtlockd"]
    ));
    config_operation!(run(
        "arch-chroot",
        &["/mnt", "systemctl", "enable", "virtnodedevd"],
    ));
    config_operation!(run(
        "arch-chroot",
        &["/mnt", "systemctl", "enable", "virtsecretd"],
    ));
    config_operation!(run(
        "arch-chroot",
        &[
            "/mnt",
            "setfacl",
            "-m",
            "u:libvirt-qemu:rwx",
            &format!("/home/{username}"),
        ],
    ));
    config_operation!(run(
        "arch-chroot",
        &[
            "/mnt",
            "flatpak",
            "override",
            "--filesystem=home",
            "org.virt_manager.virt-manager",
        ],
    ));

    cancel_with_cleanup_if_requested(cancel_requested, &swap_part)?;
    report(&mut callback, 98, 0, 0, "Finalizing installation...");
    installation_operation!(finalize_target());
    cancel_with_cleanup_if_requested(cancel_requested, &swap_part)?;
    finish_cancellable_installation();
    report(&mut callback, 100, 0, 0, "Installation complete");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{decode_lsblk_value, field, transaction_counter, verification_stage};

    #[test]
    fn lsblk_fields_match_complete_keys_and_decode_spaces() {
        let line = r#"NAME="/dev/sda" KNAME="sda" LABEL="Fluff\x20Linux""#;

        assert_eq!(field(line, "NAME"), "/dev/sda");
        assert_eq!(field(line, "LABEL"), "Fluff Linux");
        assert_eq!(field(line, "ME"), "");
        assert_eq!(decode_lsblk_value(r"USB\x20Storage"), "USB Storage");
    }

    #[test]
    fn transaction_counter_uses_the_latest_valid_counter() {
        let line = "(1304/1304) checking keys (183/1304) checking package integrity";

        assert_eq!(transaction_counter(line), Some((183, 1304)));
        assert_eq!(transaction_counter("(15/10) invalid"), None);
    }

    #[test]
    fn verification_stage_distinguishes_both_integrity_passes() {
        let first = verification_stage("(183/1304) checking package integrity").unwrap();
        let second = verification_stage("(42/1304) loading package files").unwrap();

        assert_eq!(first.4, Some((1, 2)));
        assert_eq!(second.4, Some((2, 2)));
    }
}

//! Connects the QML interface to drive discovery and installation work.

use crate::installer;
use cxx_qt::Threading;
use cxx_qt_lib::QString;
use std::collections::HashSet;
use std::pin::Pin;
use std::process::Command;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

struct Drive {
    device: String,
    model: String,
    serial: String,
    size: String,
    size_bytes: u64,
    eligible: bool,
    icon_type: String,
    media_type: String,
    partitions: String,
}

// This bridge exposes the small amount of Rust state that QML needs. Disk
// work stays in Rust so the interface never has to construct shell commands.
#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qproperty(i32, overall_progress, cxx_name = "overallProgress")]
        #[qproperty(i32, completed_items, cxx_name = "completedItems")]
        #[qproperty(i32, total_items, cxx_name = "totalItems")]
        #[qproperty(QString, status_message, cxx_name = "statusMessage")]
        #[qproperty(QString, detail_message, cxx_name = "detailMessage")]
        #[qproperty(QString, error_message, cxx_name = "errorMessage")]
        #[qproperty(bool, installing)]
        #[qproperty(bool, cancelling)]
        #[qproperty(bool, finished)]
        type InstallerBackend = super::InstallerBackendRust;

        #[qinvokable]
        #[cxx_name = "listDrivesData"]
        fn list_drives_data(&self) -> QString;

        #[qinvokable]
        #[cxx_name = "generateHostname"]
        fn generate_hostname(&self) -> QString;

        #[qinvokable]
        #[cxx_name = "installationPowerWarning"]
        fn installation_power_warning(&self) -> QString;

        #[qinvokable]
        #[cxx_name = "startInstallation"]
        fn start_installation(
            self: Pin<&mut InstallerBackend>,
            target_disk: &QString,
            hostname: &QString,
        );

        #[qinvokable]
        #[cxx_name = "cancelInstallation"]
        fn cancel_installation(self: Pin<&mut InstallerBackend>);

        #[qinvokable]
        #[cxx_name = "rebootSystem"]
        fn reboot_system(&self);

        #[qinvokable]
        #[cxx_name = "shutdownSystem"]
        fn shutdown_system(&self);
    }

    impl cxx_qt::Threading for InstallerBackend {}
}

pub struct InstallerBackendRust {
    overall_progress: i32,
    completed_items: i32,
    total_items: i32,
    status_message: QString,
    detail_message: QString,
    error_message: QString,
    installing: bool,
    cancelling: bool,
    finished: bool,
    cancel_requested: Arc<AtomicBool>,
}

impl Default for InstallerBackendRust {
    fn default() -> Self {
        Self {
            overall_progress: 0,
            completed_items: 0,
            total_items: 0,
            status_message: QString::from("Ready"),
            detail_message: QString::default(),
            error_message: QString::default(),
            installing: false,
            cancelling: false,
            finished: false,
            cancel_requested: Arc::new(AtomicBool::new(false)),
        }
    }
}

impl qobject::InstallerBackend {
    pub fn list_drives_data(&self) -> QString {
        QString::from(&discover_drives_data())
    }

    pub fn generate_hostname(&self) -> QString {
        QString::from(&random_hostname())
    }

    pub fn installation_power_warning(&self) -> QString {
        QString::from(&power_warning())
    }

    pub fn start_installation(mut self: Pin<&mut Self>, target_disk: &QString, hostname: &QString) {
        if *self.installing() || *self.finished() {
            return;
        }

        let target_disk = target_disk.to_string();
        let hostname = hostname.to_string();
        self.cancel_requested.store(false, Ordering::Relaxed);
        self.as_mut().set_installing(true);
        self.as_mut().set_cancelling(false);
        self.as_mut().set_finished(false);
        self.as_mut().set_error_message(QString::default());
        self.as_mut().set_overall_progress(0);
        self.as_mut().set_completed_items(0);
        self.as_mut().set_total_items(0);
        self.as_mut().set_detail_message(QString::default());
        self.as_mut()
            .set_status_message(QString::from("Preparing..."));

        // The installation runs away from the interface thread. Progress is
        // sent back through the Qt queue so the window remains responsive.
        let qt_thread = self.qt_thread();
        let cancel_requested = Arc::clone(&self.cancel_requested);
        std::thread::spawn(move || {
            let result = installer::install(
                &target_disk,
                &hostname,
                cancel_requested.as_ref(),
                |progress| {
                    let _ = qt_thread.queue(move |mut backend| {
                        backend.as_mut().set_overall_progress(progress.overall);
                        backend.as_mut().set_completed_items(progress.completed);
                        backend.as_mut().set_total_items(progress.total);
                        backend
                            .as_mut()
                            .set_status_message(QString::from(&progress.status));
                        backend
                            .as_mut()
                            .set_detail_message(QString::from(&progress.detail));
                    });
                },
            );

            let _ = qt_thread.queue(move |mut backend| {
                backend.as_mut().set_installing(false);
                backend.as_mut().set_cancelling(false);
                match result {
                    Ok(()) => {
                        backend.as_mut().set_finished(true);
                        backend.as_mut().set_overall_progress(100);
                        backend
                            .as_mut()
                            .set_status_message(QString::from("Installation complete"));
                    }
                    Err(error) => {
                        backend.as_mut().set_finished(false);
                        backend.as_mut().set_error_message(QString::from(&error));
                        let status = if error.starts_with("Installation cancelled.") {
                            "Installation cancelled"
                        } else {
                            "Installation failed"
                        };
                        backend.as_mut().set_status_message(QString::from(status));
                    }
                }
            });
        });
    }

    pub fn cancel_installation(mut self: Pin<&mut Self>) {
        if !*self.installing() || *self.cancelling() {
            return;
        }
        self.cancel_requested.store(true, Ordering::Relaxed);
        self.as_mut().set_cancelling(true);
        self.as_mut()
            .set_status_message(QString::from("Cancelling installation..."));
        installer::request_installation_cancellation();
        std::thread::spawn(installer::cancel_active_installation_process);
    }

    pub fn reboot_system(&self) {
        let _ = Command::new("sudo").arg("reboot").spawn();
    }

    pub fn shutdown_system(&self) {
        let _ = Command::new("sudo").args(["shutdown", "now"]).spawn();
    }
}

fn power_warning() -> String {
    let Ok(entries) = std::fs::read_dir("/sys/class/power_supply") else {
        return String::new();
    };

    let mut battery_levels = Vec::new();
    let mut battery_is_charging = false;
    let mut external_power_connected = false;

    for entry in entries.flatten() {
        let path = entry.path();
        let supply_type = std::fs::read_to_string(path.join("type"))
            .unwrap_or_default()
            .trim()
            .to_string();

        if supply_type == "Battery" {
            if let Ok(capacity) = std::fs::read_to_string(path.join("capacity"))
                .unwrap_or_default()
                .trim()
                .parse::<u8>()
            {
                battery_levels.push(capacity);
            }

            let status = std::fs::read_to_string(path.join("status"))
                .unwrap_or_default()
                .trim()
                .to_string();
            battery_is_charging |= status == "Charging" || status == "Full";
        } else {
            external_power_connected |= std::fs::read_to_string(path.join("online"))
                .unwrap_or_default()
                .trim()
                == "1";
        }
    }

    let Some(battery_level) = battery_levels.into_iter().min() else {
        return String::new();
    };

    if battery_level < 10 && !external_power_connected && !battery_is_charging {
        "Connect the charger to continue. At least 10% battery is required when the system is not connected to power.".to_string()
    } else {
        String::new()
    }
}

fn field(line: &str, key: &str) -> String {
    // lsblk pairs output is deliberately used instead of JSON so drive
    // discovery does not require a serialization crate.
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

fn human_size(bytes: u64) -> String {
    const GIB: u64 = 1024 * 1024 * 1024;
    const TIB: u64 = GIB * 1024;
    if bytes >= TIB {
        format!("{:.1} TiB", bytes as f64 / TIB as f64)
    } else {
        format!("{} GiB", bytes / GIB)
    }
}

fn human_partition_size(bytes: u64) -> String {
    const MIB: u64 = 1024 * 1024;
    const GIB: u64 = MIB * 1024;
    const TIB: u64 = GIB * 1024;
    if bytes >= TIB {
        format!("{:.1} TiB", bytes as f64 / TIB as f64)
    } else if bytes >= GIB {
        format!("{:.1} GiB", bytes as f64 / GIB as f64)
    } else {
        format!("{} MiB", bytes / MIB)
    }
}

fn discover_drives_data() -> String {
    let output = match Command::new("lsblk")
        .args([
            "-d",
            "-b",
            "-P",
            "--output",
            "NAME,MODEL,SERIAL,SIZE,TYPE,TRAN,ROTA,RM",
            "--noheadings",
        ])
        .output()
    {
        Ok(output) => output,
        Err(error) => {
            return format!("ERROR\tCould not inspect drives: {error}");
        }
    };

    let source_disks = live_source_disks();
    let drives = String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| {
            if field(line, "TYPE") != "disk" {
                return None;
            }
            let device = field(line, "NAME");
            if device.starts_with("zram") {
                return None;
            }
            if source_disks.contains(&device) {
                return None;
            }
            let size_bytes = field(line, "SIZE").parse().unwrap_or(0);
            let model = field(line, "MODEL");
            let serial = field(line, "SERIAL");
            let transport = field(line, "TRAN");
            let rotational = field(line, "ROTA");
            let removable = field(line, "RM") == "1";
            let (icon_type, media_type) = if device.starts_with("mmcblk") {
                match mmc_media_type(&device).as_str() {
                    "SD" => ("sd", "SD Card"),
                    _ => ("mmc", "MMC Storage"),
                }
            } else if transport == "usb" && rotational == "0" {
                ("usb", "USB Storage")
            } else if transport == "usb" && rotational == "1" {
                ("usb_hdd", "USB Storage")
            } else if transport == "usb" {
                ("other", "Other")
            } else if removable {
                ("removable", "Removable Media")
            } else if transport == "nvme" || device.starts_with("nvme") {
                ("ssd", "NVMe SSD")
            } else if rotational == "0" {
                ("ssd", "SSD")
            } else if rotational == "1" {
                ("hdd", "HDD")
            } else {
                ("other", "Other")
            };
            Some(Drive {
                partitions: partition_summary(&device),
                device,
                model: if model.is_empty() {
                    "Unknown Drive".to_string()
                } else {
                    model
                },
                serial: if serial.is_empty() {
                    "Not available".to_string()
                } else {
                    serial
                },
                size: human_size(size_bytes),
                size_bytes,
                eligible: size_bytes >= 20 * 1024 * 1024 * 1024,
                icon_type: icon_type.to_string(),
                media_type: media_type.to_string(),
            })
        })
        .collect::<Vec<_>>();

    drives
        .into_iter()
        .map(|drive| {
            format!(
                "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
                drive.device,
                drive.model.replace('\t', " ").replace('\n', " "),
                drive.serial.replace('\t', " ").replace('\n', " "),
                drive.size,
                drive.size_bytes,
                if drive.eligible { "1" } else { "0" },
                drive.icon_type,
                drive.media_type,
                drive.partitions.replace('\t', " ").replace('\n', " ")
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn mmc_media_type(device: &str) -> String {
    std::fs::read_to_string(format!("/sys/block/{device}/device/type"))
        .unwrap_or_default()
        .trim()
        .to_ascii_uppercase()
}

fn live_source_disks() -> HashSet<String> {
    // The live medium must never appear as an installation target. Archiso
    // provides two ways to identify it, so both are checked for reliability.
    let mut sources = Vec::new();

    if let Ok(output) = Command::new("findmnt")
        .args(["-n", "-o", "SOURCE", "--target", "/run/archiso/bootmnt"])
        .output()
    {
        let source = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if source.starts_with("/dev/") {
            sources.push(source);
        }
    }

    if let Ok(used_devices) = std::fs::read_to_string("/run/archiso/used_block_devices") {
        sources.extend(
            used_devices
                .lines()
                .map(str::trim)
                .filter(|source| source.starts_with("/dev/"))
                .map(str::to_string),
        );
    }

    let mut disks = HashSet::new();
    for source in sources {
        if let Ok(output) = Command::new("lsblk")
            .args(["-s", "-n", "-o", "NAME,TYPE", &source])
            .output()
        {
            for line in String::from_utf8_lossy(&output.stdout).lines() {
                let mut fields = line.split_whitespace();
                if let (Some(name), Some("disk")) = (fields.next(), fields.next()) {
                    disks.insert(name.to_string());
                }
            }
        }
    }
    disks
}

fn partition_summary(device: &str) -> String {
    let target = format!("/dev/{device}");
    let output = match Command::new("lsblk")
        .args([
            "-n",
            "-b",
            "-P",
            "-o",
            "NAME,TYPE,FSTYPE,LABEL,SIZE,PARTTYPE",
            &target,
        ])
        .output()
    {
        Ok(output) => output,
        Err(_) => return String::new(),
    };

    let mut saw_partition = false;
    let mut interesting = Vec::new();
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        if field(line, "TYPE") != "part" {
            continue;
        }
        saw_partition = true;
        let filesystem = field(line, "FSTYPE");
        if filesystem.is_empty() {
            continue;
        }
        let label = field(line, "LABEL");
        let partition_type = field(line, "PARTTYPE");
        // EFI and swap are expected support partitions. The interface lists
        // only partitions that can help a user recognize their data drive.
        if filesystem.eq_ignore_ascii_case("swap")
            || label.eq_ignore_ascii_case("swap")
            || label.eq_ignore_ascii_case("efi")
            || partition_type.eq_ignore_ascii_case("c12a7328-f81f-11d2-ba4b-00a0c93ec93b")
        {
            continue;
        }
        let size = human_partition_size(field(line, "SIZE").parse().unwrap_or(0));
        let filesystem = filesystem.to_ascii_uppercase();
        let partition_name = field(line, "NAME");
        if label.is_empty() {
            interesting.push(format!("{filesystem} ({size}) ({partition_name})"));
        } else {
            interesting.push(format!("{label} ({size}, {filesystem}) ({partition_name})"));
        }
    }

    if !saw_partition {
        "__UNPARTITIONED__".to_string()
    } else {
        interesting.join("|~|")
    }
}

fn random_hostname() -> String {
    use std::fs::File;
    use std::io::Read;

    let mut bytes = [0_u8; 6];
    if File::open("/dev/urandom")
        .and_then(|mut file| file.read_exact(&mut bytes))
        .is_err()
    {
        return "flufflinux".to_string();
    }

    format!(
        "FL-{}{}{}{}{}{}",
        (b'A' + bytes[0] % 26) as char,
        (b'A' + bytes[1] % 26) as char,
        bytes[2] % 10,
        bytes[3] % 10,
        bytes[4] % 10,
        bytes[5] % 10
    )
}

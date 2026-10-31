use std::env;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

struct WimSelection {
    path: PathBuf,
    drive_letter: char,
}

struct UnattendSelection {
    path: Option<PathBuf>,
}

fn main() {
    println!("==================================================");
    println!("         Windows PE Image Deployment Tool         ");
    println!("==================================================\n");

    // 1. Find WIM files across drives
    let available_wims = scan_for_wims();
    if available_wims.is_empty() {
        println!("[!] No .wim files found in root, \\sources\\ or \\images\\ folders of any drive.");
        pause_and_exit();
    }

    println!("Available WIM Images:");
    for (idx, wim) in available_wims.iter().enumerate() {
        println!("  [{}] {}", idx + 1, wim.path.display());
    }

    let wim_idx = prompt_number("Select WIM file number: ", 1, available_wims.len()) - 1;
    let selected_wim = &available_wims[wim_idx];

    // 2. Query WIM image indices using DISM
    let image_indices = get_wim_images(&selected_wim.path);
    if image_indices.is_empty() {
        println!("[!] Failed to retrieve image index list from DISM.");
        pause_and_exit();
    }

    println!("\nAvailable Images in {}", selected_wim.path.filename_str());
    for img in &image_indices {
        println!("  [{}] {}", img.index, img.name);
    }

    let img_choice = prompt_number("Select Image index to apply: ", 1, image_indices.len());
    let selected_image_index = image_indices[img_choice - 1].index;

    // 3. Find destination target disks (excluding the source drive)
    let target_disks = get_target_disks(selected_wim.drive_letter);
    if target_disks.is_empty() {
        println!("[!] No valid destination disks found.");
        pause_and_exit();
    }

    println!("\nAvailable Destination Disks:");
    for disk in &target_disks {
        println!("  [Disk {}] Size: {} GB - {}", disk.number, disk.size_gb, disk.name);
    }

    let valid_disk_nums: Vec<usize> = target_disks.iter().map(|d| d.number).collect();
    let selected_disk = prompt_disk_number("Select destination Disk number: ", &valid_disk_nums);

    // 4. Look for Unattend XML files in the source WIM's directory
    let selected_unattend = select_unattend_xml(&selected_wim.path);

    // 5. Summary & Confirmation
    println!("\n==================================================");
    println!("               CONFIRMATION DETAILS               ");
    println!("==================================================");
    println!(" Source WIM File : {}", selected_wim.path.display());
    println!(" Image Index     : {}", selected_image_index);
    println!(" Target Disk     : Disk {}", selected_disk);
    println!(" Unattend XML    : {}", match &selected_unattend.path {
        Some(p) => p.display().to_string(),
        None => "None (Skipped)".to_string(),
    });
    println!("--------------------------------------------------");
    println!(" WARNING: Disk {} WILL BE ERASED AND REPARTITIONED!", selected_disk);
    println!("==================================================");

    print!("Type 'YES' to proceed with deployment: ");
    io::stdout().flush().unwrap();
    let mut input = String::new();
    io::stdin().read_line(&mut input).unwrap();

    if input.trim() != "YES" {
        println!("Operation cancelled by user.");
        return;
    }

    // 6. Execute Deployment Steps
    println!("\n[1/4] Partitioning Disk {} via diskpart...", selected_disk);
    partition_disk(selected_disk);

    println!("\n[2/4] Applying WIM image to W:\\...");
    apply_wim_image(&selected_wim.path, selected_image_index, "W:\\");

    println!("\n[3/4] Configuring boot files via bcdboot...");
    configure_bcdboot("W:\\Windows");

    println!("\n[4/4] Copying Unattend XML if selected...");
    if let Some(xml_path) = &selected_unattend.path {
        copy_unattend_xml(xml_path, "W:\\Windows\\Panther\\Unattend.xml");
    } else {
        println!("Skipped Unattend XML step.");
    }

    // 7. Completion instruction
    println!("\n==================================================");
    println!("           DEPLOYMENT COMPLETED SUCCESSFULLY       ");
    println!("==================================================");
    println!("You can now restart your system:");
    println!("  1. Remove bootable media/USB drive.");
    println!("  2. Type 'wpeutil reboot' or 'exit' in the console.");
    println!("==================================================");
}

// --- Helper Data Structures ---

struct ImageInfo {
    index: usize,
    name: String,
}

struct DiskInfo {
    number: usize,
    size_gb: String,
    name: String,
}

trait PathExt {
    fn filename_str(&self) -> String;
}

impl PathExt for Path {
    fn filename_str(&self) -> String {
        self.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string()
    }
}

// --- Scan & Selection Logic ---

fn scan_for_wims() -> Vec<WimSelection> {
    let mut wims = Vec::new();

    for letter in b'C'..=b'Z' {
        let drive = format!("{}:\\", letter as char);
        let drive_path = Path::new(&drive);
        if !drive_path.exists() {
            continue;
        }

        let search_dirs = vec![
            drive_path.to_path_buf(),
            drive_path.join("images"),
        ];

        for dir in search_dirs {
            if let Ok(entries) = fs::read_dir(dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_file() {
                        if let Some(ext) = path.extension() {
                            if ext.to_string_lossy().eq_ignore_ascii_case("wim") {
                                wims.push(WimSelection {
                                    path,
                                    drive_letter: letter as char,
                                });
                            }
                        }
                    }
                }
            }
        }
    }
    wims
}

fn get_wim_images(wim_path: &Path) -> Vec<ImageInfo> {
    let output = Command::new("dism")
        .args(&[
            "/Get-WimInfo",
            &format!("/WimFile:{}", wim_path.display()),
        ])
        .output();

    let mut images = Vec::new();
    if let Ok(out) = output {
        let stdout = String::from_utf8_lossy(&out.stdout);
        let mut current_index = 0;

        for line in stdout.lines() {
            let line = line.trim();
            if line.starts_with("Index :") {
                if let Some(idx_str) = line.split(':').nth(1) {
                    current_index = idx_str.trim().parse::<usize>().unwrap_or(0);
                }
            } else if line.starts_with("Name :") && current_index != 0 {
                if let Some(name) = line.split(':').nth(1) {
                    images.push(ImageInfo {
                        index: current_index,
                        name: name.trim().to_string(),
                    });
                    current_index = 0;
                }
            }
        }
    }
    images
}

fn get_target_disks(source_drive_letter: char) -> Vec<DiskInfo> {
    // Generate temporary diskpart script to list disks
    let script_content = "list disk\n";
    let script_path = env::temp_dir().join("list_disks.txt");
    let _ = fs::write(&script_path, script_content);

    let output = Command::new("diskpart")
        .args(&["/s", script_path.to_str().unwrap()])
        .output();

    let _ = fs::remove_file(script_path);

    let mut disks = Vec::new();
    if let Ok(out) = output {
        let stdout = String::from_utf8_lossy(&out.stdout);
        
        // Find which disk contains source drive letter
        let source_disk = get_disk_for_drive_letter(source_drive_letter);

        for line in stdout.lines() {
            let line = line.trim();
            if line.starts_with("Disk ") {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 4 {
                    if let Ok(num) = parts[1].parse::<usize>() {
                        if Some(num) == source_disk {
                            continue; // Omit drive where WIM resides
                        }
                        
                        let size = format!("{} {}", parts[3], parts.get(4).unwrap_or(&""));
                        let name = line.to_string();
                        disks.push(DiskInfo {
                            number: num,
                            size_gb: size,
                            name,
                        });
                    }
                }
            }
        }
    }
    disks
}

fn get_disk_for_drive_letter(letter: char) -> Option<usize> {
    let script_content = format!("select volume {}\ndetail volume\n", letter);
    let script_path = env::temp_dir().join("vol_info.txt");
    let _ = fs::write(&script_path, &script_content);

    let output = Command::new("diskpart")
        .args(&["/s", script_path.to_str().unwrap()])
        .output();

    let _ = fs::remove_file(script_path);

    if let Ok(out) = output {
        let stdout = String::from_utf8_lossy(&out.stdout);
        for line in stdout.lines() {
            if line.trim().starts_with("* Disk ") || line.trim().starts_with("Disk ") {
                let parts: Vec<&str> = line.split_whitespace().collect();
                for (i, part) in parts.iter().enumerate() {
                    if *part == "Disk" {
                        if let Some(num_str) = parts.get(i + 1) {
                            if let Ok(num) = num_str.parse::<usize>() {
                                return Some(num);
                            }
                        }
                    }
                }
            }
        }
    }
    None
}

fn select_unattend_xml(wim_path: &Path) -> UnattendSelection {
    let parent_dir = match wim_path.parent() {
        Some(p) => p,
        None => return UnattendSelection { path: None },
    };

    let mut xml_files = Vec::new();
    if let Ok(entries) = fs::read_dir(parent_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                if let Some(ext) = path.extension() {
                    if ext.to_string_lossy().eq_ignore_ascii_case("xml") {
                        xml_files.push(path);
                    }
                }
            }
        }
    }

    if xml_files.is_empty() {
        return UnattendSelection { path: None };
    }

    println!("\nFound Unattend XML files in source folder:");
    println!("  [0] Do not apply any unattend file");
    for (idx, xml) in xml_files.iter().enumerate() {
        println!("  [{}] {}", idx + 1, xml.filename_str());
    }

    let choice = prompt_number("Select XML file (0 to skip): ", 0, xml_files.len());
    if choice == 0 {
        UnattendSelection { path: None }
    } else {
        UnattendSelection {
            path: Some(xml_files[choice - 1].clone()),
        }
    }
}

// --- Execution Commands ---

fn partition_disk(disk_num: usize) {
    let script = format!(
        "select disk {}\n\
         clean\n\
         convert gpt\n\
         create partition efi size=100\n\
         format quick fs=fat32 label=\"System\"\n\
         assign letter=S\n\
         create partition msr size=16\n\
         create partition primary\n\
         format quick fs=ntfs label=\"Windows\"\n\
         assign letter=W\n",
        disk_num
    );

    let script_path = env::temp_dir().join("part_script.txt");
    fs::write(&script_path, script).expect("Failed to write diskpart script");

    let status = Command::new("diskpart")
        .args(&["/s", script_path.to_str().unwrap()])
        .status()
        .expect("Failed to run diskpart");

    let _ = fs::remove_file(script_path);

    if !status.success() {
        println!("[!] Diskpart failed!");
        pause_and_exit();
    }
}

fn apply_wim_image(wim_path: &Path, index: usize, target_drive: &str) {
    let status = Command::new("dism")
        .args(&[
            "/Apply-Image",
            &format!("/ImageFile:{}", wim_path.display()),
            &format!("/Index:{}", index),
            &format!("/ApplyDir:{}", target_drive),
        ])
        .status()
        .expect("Failed to execute DISM");

    if !status.success() {
        println!("[!] DISM apply-image failed!");
        pause_and_exit();
    }
}

fn configure_bcdboot(windows_dir: &str) {
    let status = Command::new("bcdboot")
        .args(&[windows_dir, "/s", "S:", "/f", "UEFI"])
        .status()
        .expect("Failed to execute bcdboot");

    if !status.success() {
        println!("[!] BCDBoot setup failed!");
        pause_and_exit();
    }
}

fn copy_unattend_xml(src: &Path, dst: &str) {
    let dst_path = Path::new(dst);
    if let Some(parent) = dst_path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Err(e) = fs::copy(src, dst_path) {
        println!("[!] Warning: Failed to copy unattend XML: {}", e);
    } else {
        println!("Successfully copied unattend XML to {}", dst);
    }
}

// --- CLI Prompt Utilities ---

fn prompt_number(prompt: &str, min: usize, max: usize) -> usize {
    loop {
        print!("{}", prompt);
        io::stdout().flush().unwrap();
        let mut input = String::new();
        if io::stdin().read_line(&mut input).is_ok() {
            if let Ok(num) = input.trim().parse::<usize>() {
                if num >= min && num <= max {
                    return num;
                }
            }
        }
        println!("Invalid choice. Enter a number between {} and {}.", min, max);
    }
}

fn prompt_disk_number(prompt: &str, valid_numbers: &[usize]) -> usize {
    loop {
        print!("{}", prompt);
        io::stdout().flush().unwrap();
        let mut input = String::new();
        if io::stdin().read_line(&mut input).is_ok() {
            if let Ok(num) = input.trim().parse::<usize>() {
                if valid_numbers.contains(&num) {
                    return num;
                }
            }
        }
        println!("Invalid selection. Please enter a disk number from the list.");
    }
}

fn pause_and_exit() {
    println!("\nPress Enter to exit...");
    let mut unused = String::new();
    let _ = io::stdin().read_line(&mut unused);
    std::process::exit(1);
}

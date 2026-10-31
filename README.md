# WinPE Rust Image Deployment Tool

A lightweight, standalone console application written in **plain Rust** designed to run natively in **Windows PE (Preinstallation Environment)**. It has zero external dependencies (no Visual C++ Runtime or external DLLs required) and provides an interactive wizard to deploy Windows images (`.wim`) onto target disks.

---

## Features

* **Zero Dependencies:** Compiles statically using the MSVC target (`-C target-feature=+crt-static`), allowing it to run smoothly out-of-the-box in barebones Windows PE environments.
* **Automatic Drive & WIM Discovery:** Scans drive roots (`.\`) and `\sources\` folders across all connected drives for `.wim` files.
* **Interactive Image Selection:** Lists available images inside the selected WIM file via DISM and lets you pick the target index.
* **Smart Disk Filtering:** Queries available physical disks using `diskpart` and automatically filters out the source drive where the WIM file resides to prevent accidental formatting.
* **Unattend Support:** Optionally lets you choose an XML answer file from the source directory to automate OOBE setup.
* **Automated Deployment Pipeline:**
  1. Partitions and formats the destination disk (GPT layout: EFI system partition, MSR, and NTFS Windows partition).
  2. Applies the selected WIM image using `dism /Apply-Image`.
  3. Generates boot files using `bcdboot`.
  4. Copies the selected Unattend XML file to `W:\Windows\Panther\Unattend.xml`.

---

## Prerequisites & Building

1. **Rust MSVC Toolchain:** Ensure you have the standard Windows MSVC toolchain installed (`x86_64-pc-windows-msvc`).
2. **Static Linking Configuration:** Create a file named `.cargo/config.toml` in your project root to ensure it links statically without requiring external CRTs:
   ```toml
   [target.x86_64-pc-windows-msvc]
   rustflags = ["-C", "target-feature=+crt-static"]
   ```
3. **Build Release Binary:**
   ```bash
   cargo build --release
   ```
   The compiled executable will be located at `target\release\windeploy.exe`. Copy this binary onto your WinPE bootable media or network share.

---

## Limitations & Known Disadvantages

While fully functional for standard image deployment workflows, please take note of the following current limitations:

* **Strict WIM File Placement:** WIM files must be located strictly in either the root directory (`.\`) or the `\sources\` folder of a detected drive. Nested sub-folders are not currently scanned.
* **Unattend XML Placement:** XML unattend files **must** reside in the exact same source folder as the selected WIM file to be detected by the app.
* **WIM Only (No SWM Support):** Split WIM (`.swm`) files are not supported yet; only single monolithic `.wim` archives can be processed.
* **No WinRE Apply Support:** The tool currently targets primary Windows OS installations and does not include logic to apply or configure a dedicated Windows Recovery Environment (`WinRE.wim`) partition.

---

## User Guide

### 1. Preparing the Bootable Media
Place your compiled executable (e.g., `windeploy.exe`) along with your `.wim` images on a USB drive or WinPE image directory according to the required folder structure:

```text
E:\ (USB Drive Root)
│   windeploy.exe
│   install.wim
│   unattend.xml       <-- Must be in the same directory as install.wim
│
└───sources\
        custom_win11.wim
        autounattend.xml <-- Must be in the same directory as custom_win11.wim
```

### 2. Running in Windows PE
1. Boot the target machine into **Windows PE**.
2. Navigate to your USB drive letter or network share:
   ```cmd
   D:
   windeploy.exe
   ```

### 3. Step-by-Step Deployment Steps

1. **Select WIM File:** The tool scans all mounted drives and lists available `.wim` files with numbered indexes. Input the number corresponding to your desired image.
2. **Select Image Index:** The app queries the selected WIM using DISM and displays all internal image indexes (e.g., *Windows 11 Pro*, *Windows 11 Enterprise*). Pick the required index.
3. **Select Destination Disk:** The application uses `diskpart` to detect target physical disks. The physical disk hosting your source WIM is automatically excluded to prevent accidental destruction of your deployment media. Select the disk number you wish to format and install Windows on.
4. **Select Unattend XML (Optional):** If any `.xml` files are found in the same folder as the WIM file, they will be listed. Select one to copy to `C:\Windows\Panther\Unattend.xml` or choose `0` to skip.
5. **Review Confirmation:** A summary screen displays your chosen settings along with a warning.
6. **Confirm Deployment:** Type `YES` to start the partition, apply, bcdboot, and file copy process.

### 4. Rebooting
Once deployment finishes successfully:
1. Remove your WinPE USB drive or bootable installation media.
2. Type `wpeutil reboot` in the WinPE command prompt to restart into your newly installed Windows OS.

---

## License

This project is open-source and available under the [MIT License](LICENSE).
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};
use std::{fs, thread};

use crate::{Package, PackageType};

pub struct HomebrewScanner {
    pub state: Arc<Mutex<ScanningState>>,
    pub packages: Arc<Mutex<Vec<Package>>>,
}
#[derive(Debug, Clone)]
pub struct ScanningState {
    pub packages_found: usize,
    pub packages_scanned: usize,
    pub total_packages: usize,
    pub current_path: String,
    pub start_time: Instant,
    pub is_paused: bool,
    pub scan_complete: bool,
    pub error_message: Option<String>,
}

impl ScanningState {
    pub fn new() -> Self {
        Self {
            packages_found: 0,
            packages_scanned: 0,
            total_packages: 0,
            current_path: "Initializing...".to_string(),
            start_time: Instant::now(),
            is_paused: false,
            scan_complete: false,
            error_message: None,
        }
    }

    pub fn progress_percentage(&self) -> u16 {
        if self.total_packages == 0 {
            0
        } else {
            ((self.packages_scanned as f64 / self.total_packages as f64) * 100.0) as u16
        }
    }

    pub fn elapsed_time(&self) -> Duration {
        self.start_time.elapsed()
    }

    pub fn format_elapsed(&self) -> String {
        let elapsed = self.elapsed_time();
        let mins = elapsed.as_secs() / 60;
        let secs = elapsed.as_secs() % 60;
        format!("{:02}:{:02}", mins, secs)
    }
}

impl HomebrewScanner {
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(ScanningState::new())),
            packages: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn get_homebrew_prefix() -> Result<PathBuf, String> {
        let output = Command::new("brew")
            .args(["--prefix"])
            .output()
            .map_err(|e| format!("failed to run 'brew --prefix': {}", e))?;

        if !output.status.success() {
            return Err("Hombrew not found or not properly installed.".to_string());
        }

        let prefix = String::from_utf8(output.stdout)
            .map_err(|e| format!("Invalid UTF-8 in brew --prefix output: {}", e))?
            .trim()
            .to_string();

        Ok(PathBuf::from(prefix))
    }

    fn get_installed_packages() -> Result<(Vec<String>, Vec<String>), String> {
        let formulas_output = Command::new("brew")
            .args(["list", "--formula"])
            .output()
            .map_err(|e| format!("Failed to get foruma list: {}", e))?;

        let formulas = if formulas_output.status.success() {
            String::from_utf8(formulas_output.stdout)
                .map_err(|e| format!("Invalid UTF-8 in formulas output: {}", e))?
                .lines()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect()
        } else {
            Vec::new()
        };

        let casks_output = Command::new("brew")
            .args(["list", "--cask"])
            .output()
            .map_err(|e| format!("Failed to get cask list: {}", e))?;

        let casks = if casks_output.status.success() {
            String::from_utf8(casks_output.stdout)
                .map_err(|e| format!("Invalid UTF-8 in casks output: {}", e))?
                .lines()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect()
        } else {
            Vec::new()
        };

        Ok((formulas, casks))
    }

    fn get_file_acess_info(path: &Path) -> Option<SystemTime> {
        fs::metadata(path)
            .ok()
            .and_then(|metadata| metadata.accessed().ok())
    }

    fn find_package_paths(
        prefix: &Path,
        package_name: &str,
        package_type: &PackageType,
    ) -> Vec<PathBuf> {
        let mut paths = Vec::new();

        match package_type {
            PackageType::Formula => {
                let cellar_path = prefix.join("Cellar").join(package_name);
                if cellar_path.exists() {
                    if let Ok(entries) = fs::read_dir(&cellar_path) {
                        for entry in entries.flatten() {
                            if entry.file_type().is_ok_and(|ft| ft.is_dir()) {
                                paths.push(entry.path());
                            }
                        }
                    }
                }

                let bin_path = prefix.join("bin").join(package_name);
                if bin_path.exists() {
                    paths.push(bin_path);
                }
            }
            PackageType::Cask => {
                let cask_path = prefix.join("Caskroom").join(package_name);
                if cask_path.exists() {
                    paths.push(cask_path);
                }

                if let Ok(entries) = fs::read_dir("/Applications") {
                    for entry in entries.flatten() {
                        let app_name = entry.file_name();
                        if let Some(name_str) = app_name.to_str() {
                            if name_str
                                .to_lowercase()
                                .contains(&package_name.to_lowercase())
                            {
                                paths.push(entry.path());
                            }
                        }
                    }
                }
            }
        }
        paths
    }

    fn scan_packages(&self) -> Result<(), String> {
        {
            let mut state = self.state.lock().unwrap();
            state.current_path = "Getting Hombrew prefix...".to_string();
        }

        let prefix = Self::get_homebrew_prefix()?;

        {
            let mut state = self.state.lock().unwrap();
            state.current_path = "Getting package list...".to_string();
        }

        let (formulas, casks) = Self::get_installed_packages()?;

        {
            let mut state = self.state.lock().unwrap();
            state.total_packages = formulas.len() + casks.len();
        }

        let mut all_packages = Vec::new();

        for (i, formula) in formulas.iter().enumerate() {
            {
                let state = self.state.lock().unwrap();
                if state.is_paused && !state.scan_complete {
                    break;
                }

                thread::sleep(Duration::from_millis(100));
            }

            {
                let mut state = self.state.lock().unwrap();
                state.packages_scanned = i + 1;
                state.current_path = format!("Scanning formula: {}", formula);
            }

            let paths = Self::find_package_paths(&prefix, formula, &PackageType::Formula);
            let (last_accessed, last_accessed_path) = if let Some(path) = paths.first() {
                (
                    Self::get_file_acess_info(path),
                    Some(path.to_string_lossy().to_string()),
                )
            } else {
                (None, None)
            };

            let package = Package {
                name: formula.clone(),
                package_type: PackageType::Formula,
                last_accessed,
                last_accessed_path,
            };

            all_packages.push(package);

            {
                let mut state = self.state.lock().unwrap();
                state.packages_found = all_packages.len();
            }
        }

        for (i, cask) in casks.iter().enumerate() {
            {
                let state = self.state.lock().unwrap();
                if state.is_paused && !state.scan_complete {
                    break;
                }
                thread::sleep(Duration::from_millis(100));
            }

            {
                let mut state = self.state.lock().unwrap();
                state.packages_scanned = formulas.len() + i + 1;
                state.current_path = format!("Scanning cask: {}", cask);
            }

            let paths = Self::find_package_paths(&prefix, cask, &PackageType::Cask);
            let (last_accessed, last_accessed_path) = if let Some(path) = paths.first() {
                (
                    Self::get_file_acess_info(path),
                    Some(path.to_string_lossy().to_string()),
                )
            } else {
                (None, None)
            };

            let package = Package {
                name: cask.clone(),
                package_type: PackageType::Cask,
                last_accessed,
                last_accessed_path,
            };

            all_packages.push(package);

            {
                let mut state = self.state.lock().unwrap();
                state.packages_found = all_packages.len();
            }
        }

        {
            let mut packages = self.packages.lock().unwrap();
            packages.clear();
            packages.extend(all_packages);
        }

        {
            let mut state = self.state.lock().unwrap();
            state.scan_complete = true;
            state.current_path = "Scan complete!".to_string();
        }
        Ok(())
    }

    pub fn start_scan(&self) -> thread::JoinHandle<()> {
        let scanner = HomebrewScanner {
            state: Arc::clone(&self.state),
            packages: Arc::clone(&self.packages),
        };

        thread::spawn(move || {
            if let Err(e) = scanner.scan_packages() {
                let mut state = scanner.state.lock().unwrap();
                state.error_message = Some(e);
                state.scan_complete = true;
            }
        })
    }

    pub fn get_state(&self) -> ScanningState {
        self.state.lock().unwrap().clone()
    }

    pub fn get_packages(&self) -> Vec<Package> {
        self.packages.lock().unwrap().clone()
    }

    pub fn toggle_pause(&self) {
        let mut state = self.state.lock().unwrap();
        state.is_paused = !state.is_paused;
    }

    pub fn delete_package_with_output(
        package: &Package,
        output_sender: mpsc::Sender<String>,
    ) -> Result<(), String> {
        let package_arg = match package.package_type {
            PackageType::Formula => "--formula",
            PackageType::Cask => "--cask",
        };

        // Send initial command info
        let command_line = format!("$ brew uninstall {} {}", package_arg, package.name);
        let _ = output_sender.send(command_line);
        let _ = output_sender.send("".to_string()); // Empty line

        // Start the brew uninstall process with piped output
        let mut child = Command::new("brew")
            .args(["uninstall", package_arg, &package.name])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("Failed to start brew uninstall: {}", e))?;

        // Read stdout in real-time
        if let Some(stdout) = child.stdout.take() {
            let reader = BufReader::new(stdout);
            for line in reader.lines() {
                match line {
                    Ok(line_content) => {
                        let _ = output_sender.send(line_content);
                    }
                    Err(_) => break,
                }
            }
        }

        // Wait for the process to complete
        let exit_status = child
            .wait()
            .map_err(|e| format!("Failed to wait for brew process: {}", e))?;

        if !exit_status.success() {
            // Read stderr if the command failed
            if let Some(stderr) = child.stderr.take() {
                let reader = BufReader::new(stderr);
                for line_result in reader.lines() {
                    match line_result {
                        Ok(line_content) => {
                            let _ = output_sender.send(line_content);
                        }
                        Err(_) => break, // Stop reading on any IO error
                    }
                }
            }
            return Err(format!(
                "brew uninstall failed with exit code: {:?}",
                exit_status.code()
            ));
        }

        let _ = output_sender.send("".to_string()); // Empty line
        let _ = output_sender.send("✅ Uninstall completed successfully!".to_string());

        Ok(())
    }
}

use chrono::{DateTime, Local};
use colored::*;
use dialoguer::{theme::ColorfulTheme, MultiSelect};
use rayon::prelude::*;
use std::collections::HashMap;
use std::fs;
use std::io::{self, Write};
use std::path::Path;
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone)]
struct Package {
    name: String,
    package_type: PackageType,
    last_accessed: Option<SystemTime>,
    last_accessed_path: Option<String>,
}

#[derive(Debug, PartialEq, Clone)]
enum PackageType {
    Formula,
    Cask,
}

fn main() -> io::Result<()> {
    println!("{}", "===================================".blue().bold());
    println!("{}", "    Homebrew Usage Tracker v1.0    ".blue().bold());
    println!("{}", "===================================".blue().bold());

    // Check if Homebrew is installed
    if !is_homebrew_installed() {
        println!("{}", "Error: Homebrew is not installed.".red().bold());
        return Ok(());
    }

    println!("{}", "Scanning Homebrew packages...".yellow());

    // Get all formulas and casks in parallel
    let (formulas, casks) = rayon::join(
        || get_brew_packages("list", "formula"),
        || get_brew_packages("list", "cask"),
    );

    let formulas = formulas?;
    let casks = casks?;

    println!("{} formulas found", formulas.len());
    println!("{} casks found", casks.len());

    // Pre-compute common paths
    let brew_prefix = get_brew_prefix()?;
    println!("{}", "Building path cache...".yellow());
    let path_cache = build_path_cache(&brew_prefix)?;

    // Process packages in parallel
    println!(
        "{}",
        "Analyzing package usage (this may take a moment)...".yellow()
    );

    // Create progress counter
    let progress_counter = Arc::new(Mutex::new(0));
    let total_packages = formulas.len() + casks.len();

    // Process formulas and casks in parallel
    let formula_packages: Vec<Package> = formulas
        .par_iter()
        .map(|formula| {
            let package = analyze_package_with_cache(
                formula,
                PackageType::Formula,
                &brew_prefix,
                &path_cache,
            );

            // Update progress
            let mut counter = progress_counter.lock().unwrap();
            *counter += 1;
            if *counter % 10 == 0 || *counter == total_packages {
                print!(
                    "\rProgress: {}/{} packages analyzed",
                    *counter, total_packages
                );
                io::stdout().flush().unwrap_or(());
            }

            package
        })
        .collect();

    let cask_packages: Vec<Package> = casks
        .par_iter()
        .map(|cask| {
            let package =
                analyze_package_with_cache(cask, PackageType::Cask, &brew_prefix, &path_cache);

            // Update progress
            let mut counter = progress_counter.lock().unwrap();
            *counter += 1;
            if *counter % 10 == 0 || *counter == total_packages {
                print!(
                    "\rProgress: {}/{} packages analyzed",
                    *counter, total_packages
                );
                io::stdout().flush().unwrap_or(());
            }

            package
        })
        .collect();

    println!(); // End the progress line

    // Combine and sort packages
    let mut packages = Vec::with_capacity(formula_packages.len() + cask_packages.len());
    packages.extend(formula_packages);
    packages.extend(cask_packages);

    // Sort packages by last accessed time (oldest first)
    packages.sort_by_key(|p| p.last_accessed.map_or(UNIX_EPOCH, |t| t));

    // Display results
    display_packages(&packages);

    // Interactive removal
    offer_package_removal(&packages)?;

    Ok(())
}

fn is_homebrew_installed() -> bool {
    Command::new("brew").arg("--version").output().is_ok()
}

fn get_brew_packages(command: &str, package_type: &str) -> io::Result<Vec<String>> {
    let output = Command::new("brew")
        .args(&[command.to_string(), "--".to_owned() + package_type])
        .output()?;

    if output.status.success() {
        let output_str = String::from_utf8_lossy(&output.stdout);
        Ok(output_str.lines().map(|s| s.to_string()).collect())
    } else {
        eprintln!(
            "Error running brew command: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        Ok(Vec::new())
    }
}

/// Pre-build a cache of file paths and their access times
fn build_path_cache(brew_prefix: &str) -> io::Result<HashMap<String, (SystemTime, String)>> {
    let mut cache = HashMap::new();

    // Common directories for formulas
    let formula_paths = [
        format!("{}/opt", brew_prefix),
        format!("{}/bin", brew_prefix),
    ];

    // Common directories for casks
    let cask_paths = [
        "/Applications".to_string(),
        "/Applications/Homebrew Cask".to_string(),
        shellexpand::tilde("~/Applications").into_owned(),
    ];

    // Process all paths
    for path_str in formula_paths.iter().chain(cask_paths.iter()) {
        let path = Path::new(path_str);
        if !path.exists() || !path.is_dir() {
            continue;
        }

        scan_directory_for_cache(path, &mut cache)?;
    }

    Ok(cache)
}

/// Scan directory and build cache of access times
fn scan_directory_for_cache(
    dir_path: &Path,
    cache: &mut HashMap<String, (SystemTime, String)>,
) -> io::Result<()> {
    if !dir_path.exists() || !dir_path.is_dir() {
        return Ok(());
    }

    let entries = match fs::read_dir(dir_path) {
        Ok(entries) => entries,
        Err(_) => return Ok(()), // Skip directories we can't read
    };

    for entry in entries {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue, // Skip entries we can't read
        };

        let path = entry.path();
        let path_str = path.to_string_lossy().to_string();

        // Get access time
        if let Ok(metadata) = fs::metadata(&path) {
            if let Ok(accessed) = metadata.accessed() {
                // Store info in cache with path as key for quick lookups
                cache.insert(path_str.clone(), (accessed, path_str));
            }
        }

        // Recursively process directories (but not too deep)
        if path.is_dir() && path.components().count() < 10 {
            // Only go 10 levels deep to avoid excessive recursion
            let _ = scan_directory_for_cache(&path, cache);
        }
    }

    Ok(())
}

/// Analyze a package using the pre-built cache
fn analyze_package_with_cache(
    name: &str,
    package_type: PackageType,
    brew_prefix: &str,
    path_cache: &HashMap<String, (SystemTime, String)>,
) -> Package {
    let mut package = Package {
        name: name.to_string(),
        package_type: package_type.clone(),
        last_accessed: None,
        last_accessed_path: None,
    };

    // Determine paths to check based on package type
    let paths_to_check = match package_type {
        PackageType::Formula => {
            vec![
                format!("{}/opt/{}/bin", brew_prefix, name),
                format!("{}/opt/{}", brew_prefix, name),
                format!("{}/bin/{}", brew_prefix, name),
            ]
        }
        PackageType::Cask => {
            vec![
                format!("/Applications/{}.app", name),
                format!("/Applications/Homebrew Cask/{}.app", name),
                format!("~/Applications/{}.app", name).replace("~", &shellexpand::tilde("~")),
            ]
        }
    };

    // Check the cache for each path
    for path_str in paths_to_check {
        // Check exact path match
        if let Some((access_time, file_path)) = path_cache.get(&path_str) {
            if package.last_accessed.is_none() || package.last_accessed.unwrap() < *access_time {
                package.last_accessed = Some(*access_time);
                package.last_accessed_path = Some(file_path.clone());
            }
        }

        // Also check for paths that start with this path (for subdirectories)
        for (cache_path, (access_time, file_path)) in path_cache.iter() {
            if cache_path.starts_with(&path_str)
                && (package.last_accessed.is_none()
                    || package.last_accessed.unwrap() < *access_time)
            {
                package.last_accessed = Some(*access_time);
                package.last_accessed_path = Some(file_path.clone());
            }
        }
    }

    package
}

fn get_brew_prefix() -> io::Result<String> {
    let output = Command::new("brew").args(["--prefix"]).output()?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        Err(io::Error::new(
            io::ErrorKind::Other,
            "Failed to get brew prefix",
        ))
    }
}

fn format_time(time: Option<SystemTime>) -> String {
    match time {
        Some(time) => {
            let duration = time.duration_since(UNIX_EPOCH).unwrap_or_default();
            let datetime =
                DateTime::from_timestamp(duration.as_secs() as i64, 0).unwrap_or_default();
            let local_time: DateTime<Local> = DateTime::from(datetime);
            local_time.format("%Y-%m-%d %H:%M:%S").to_string()
        }
        None => "Unknown".to_string(),
    }
}

fn display_packages(packages: &[Package]) {
    println!(
        "\n{}",
        "=== Packages sorted by last access time (oldest first) ==="
            .green()
            .bold()
    );
    println!(
        "{:<30} {:<10} {:<30} {:<50}",
        "Package Name".cyan().bold(),
        "Type".cyan().bold(),
        "Last Accessed".cyan().bold(),
        "Path".cyan().bold()
    );
    println!("{}", "=".repeat(120).dimmed());

    for package in packages {
        let package_type = match package.package_type {
            PackageType::Formula => "Formula".yellow(),
            PackageType::Cask => "Cask".blue(),
        };

        let last_accessed = format_time(package.last_accessed);
        let last_accessed_colored = if last_accessed == "Unknown" {
            last_accessed.red()
        } else {
            last_accessed.normal()
        };

        println!(
            "{:<30} {:<10} {:<30} {:<50}",
            package.name,
            package_type,
            last_accessed_colored,
            package
                .last_accessed_path
                .as_deref()
                .unwrap_or("Unknown")
                .dimmed()
        );
    }
}

fn offer_package_removal(packages: &[Package]) -> io::Result<()> {
    println!(
        "\n{}",
        "Would you like to remove any of the least used packages?"
            .yellow()
            .bold()
    );

    // Get top 20 least used packages
    let candidates: Vec<&Package> = packages
        .iter()
        .filter(|p| p.last_accessed.is_some()) // Filter out packages with unknown access times
        .take(20) // Take top 20 least recently used
        .collect();

    if candidates.is_empty() {
        println!("No eligible packages found for removal.");
        return Ok(());
    }

    let items: Vec<String> = candidates
        .iter()
        .map(|p| {
            let package_type = match p.package_type {
                PackageType::Formula => "Formula",
                PackageType::Cask => "Cask",
            };
            format!(
                "{} ({}) - Last accessed: {}",
                p.name,
                package_type,
                format_time(p.last_accessed)
            )
        })
        .collect();

    let selection = MultiSelect::with_theme(&ColorfulTheme::default())
        .with_prompt("Select packages to remove (Space to select, Enter to confirm)")
        .items(&items)
        .interact()?;

    if selection.is_empty() {
        println!("No packages selected for removal.");
        return Ok(());
    }

    println!("{}", "The following packages will be removed:".red().bold());
    for &index in &selection {
        println!("- {}", items[index]);
    }

    print!("Proceed with removal? (y/n): ");
    io::stdout().flush()?;

    let mut response = String::new();
    io::stdin().read_line(&mut response)?;

    if response.trim().to_lowercase() == "y" {
        for &index in &selection {
            let package = candidates[index];
            let package_type_arg = match package.package_type {
                PackageType::Formula => "",
                PackageType::Cask => "--cask",
            };

            println!("Removing {} {}...", package_type_arg, package.name.yellow());

            let args = if package_type_arg.is_empty() {
                vec!["uninstall", &package.name]
            } else {
                vec!["uninstall", package_type_arg, &package.name]
            };

            let output = Command::new("brew").args(&args).output()?;

            if output.status.success() {
                println!(
                    "{} {}",
                    package.name.green(),
                    "successfully removed".green()
                );
            } else {
                println!(
                    "{} {}:\n{}",
                    "Error removing".red(),
                    package.name.yellow(),
                    String::from_utf8_lossy(&output.stderr)
                );
            }
        }
    } else {
        println!("Operation cancelled.");
    }

    Ok(())
}

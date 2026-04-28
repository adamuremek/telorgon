use std::env;
use std::path::{Path, PathBuf};

fn main() {
    if let Err(error) = run() {
        eprintln!("lithic-theme-build: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = env::args().skip(1);
    let theme_dir = args.next().map(PathBuf::from).ok_or_else(|| {
        "usage: lithic-theme-build <theme_dir> [output.lithic-theme]".to_string()
    })?;
    let output = args.next().map(PathBuf::from).unwrap_or_else(|| {
        let name = theme_dir
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("theme");
        theme_dir
            .join("target")
            .join(format!("{name}.lithic-theme"))
    });
    if args.next().is_some() {
        return Err("usage: lithic-theme-build <theme_dir> [output.lithic-theme]".into());
    }

    let manifest = read_to_string(theme_dir.join("theme.toml").as_path())?;
    let recipe = read_to_string(theme_dir.join("theme.recipe").as_path())?;
    lithic_theme::write_packed_theme(
        &manifest,
        &recipe,
        theme_dir.join("assets").as_path(),
        &output,
    )?;
    println!("{}", output.display());
    Ok(())
}

fn read_to_string(path: &Path) -> Result<String, Box<dyn std::error::Error>> {
    std::fs::read_to_string(path)
        .map_err(|error| format!("failed to read {}: {error}", path.display()).into())
}

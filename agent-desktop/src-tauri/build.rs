use std::path::Path;

fn main() {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let release_dir = manifest_dir
        .join("binaries")
        .join("code-server")
        .join("release");
    let entry = release_dir.join("out").join("node").join("entry.js");

    // 检查 code-server 是否已就绪
    if !entry.exists() {
        println!("cargo:warning=code-server not found, attempting auto-setup...");
        println!("cargo:warning=If auto-setup fails, please run:");
        println!("cargo:warning=  npm run download:code-server");
        attempt_setup(&release_dir);
    } else if !release_dir.join("node_modules").exists() {
        println!("cargo:warning=node_modules not found, running npm install...");
        run_npm_install(&release_dir);
    }

    tauri_build::build();
}

fn attempt_setup(release_dir: &Path) {
    let url =
        "https://github.com/coder/code-server/releases/download/v4.127.0/package.tar.gz";
    let tarball = release_dir
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("package.tar.gz");

    std::fs::create_dir_all(release_dir).ok();

    if !tarball.exists() {
        println!("cargo:warning=Downloading package.tar.gz (~54MB)...");
        match download_file(url, &tarball) {
            Ok(size) => println!("cargo:warning=Downloaded {}MB", size / 1_048_576),
            Err(e) => {
                println!("cargo:warning=Download failed: {}", e);
                return;
            }
        }
    }

    println!("cargo:warning=Extracting package.tar.gz...");
    let status = std::process::Command::new("tar")
        .arg("-xzf")
        .arg(&tarball)
        .arg("-C")
        .arg(release_dir)
        .arg("--strip-components=1")
        .status();

    match status {
        Ok(s) if s.success() => {
            let _ = std::fs::remove_file(&tarball);
            println!("cargo:warning=Extraction complete");
        }
        _ => {
            println!("cargo:warning=Extraction failed. Please extract manually.");
            return;
        }
    }

    run_npm_install(release_dir);
}

fn run_npm_install(release_dir: &Path) {
    let status = std::process::Command::new("npm")
        .args(["install", "--production", "--ignore-scripts"])
        .current_dir(release_dir)
        .status();

    match status {
        Ok(s) if s.success() => println!("cargo:warning=npm install complete"),
        Ok(s) => println!(
            "cargo:warning=npm install exit: {}",
            s.code().unwrap_or(-1)
        ),
        Err(e) => println!("cargo:warning=npm install error: {}", e),
    }
}

fn download_file(url: &str, dest: &Path) -> Result<u64, String> {
    use std::io::Read;

    let agent = ureq::Agent::new_with_config(
        ureq::config::Config::builder()
            .timeout_connect(Some(std::time::Duration::from_secs(30)))
            .timeout_global(Some(std::time::Duration::from_secs(600)))
            .build(),
    );

    let resp = agent
        .get(url)
        .header("User-Agent", "Agent-Desktop-Build/1.0")
        .call()
        .map_err(|e| format!("Request failed: {}", e))?;

    if resp.status() != 200 {
        return Err(format!("HTTP {}", resp.status()));
    }

    let mut reader = resp.into_body().into_reader();
    let mut file =
        std::fs::File::create(dest).map_err(|e| format!("Cannot create: {}", e))?;
    let mut buf = [0u8; 65536];
    let mut downloaded: u64 = 0;

    loop {
        let n = reader
            .read(&mut buf)
            .map_err(|e| format!("Download error: {}", e))?;
        if n == 0 {
            break;
        }
        std::io::Write::write_all(&mut file, &buf[..n])
            .map_err(|e| format!("Write error: {}", e))?;
        downloaded += n as u64;
    }
    Ok(downloaded)
}

use std::collections::HashMap;
use std::path::Path;
use std::process::ExitCode;

use msixvc::layout::PAGE_SIZE;
use msixvc::xvd::{SegmentFile, XvdFile};
use tokio::fs::{File, OpenOptions};
use tokio::io::AsyncWriteExt;
use xodus::tokens::TokenManager;

use crate::license::get_license;

// Writes decrypted copies of the files `run` decrypts at launch, for Wine
// builds that cannot load them from memory. The encrypted originals are moved
// to `.xodus-encrypted/` in the game folder.
pub async fn run(
    client: &reqwest::Client,
    tokens: &TokenManager,
    source: String,
    market: Option<String>,
) -> ExitCode {
    let mut lfiles: HashMap<String, SegmentFile> = HashMap::new();

    let out: &Path = Path::new(&source);
    let final_path = out.join(".xodus-streaming.msixvc");

    let mut file = match OpenOptions::new().read(true).open(&final_path).await {
        Ok(file) => file,
        Err(err) => {
            eprintln!("{}: {}", final_path.display(), err);
            return ExitCode::FAILURE;
        }
    };

    let xvd = XvdFile::parse(&mut file).await.expect("no err");

    let files = xvd.parse_user_package_files(&mut file).await.expect("ok");
    for (k, v) in &files {
        if k == "SegmentMetadata.bin" {
            let sfiles = xvd.parse_segment_metadata(&mut file, v).await.expect("ok");
            lfiles = sfiles;
        }
    }

    // Classic files
    if lfiles.is_empty() {
        let sfiles = xvd
            .parse_ntfs_segment_metadata(&mut file, !lfiles.is_empty())
            .await
            .expect("ok");
        lfiles.extend(sfiles);
    }

    let encrypted: Vec<_> = lfiles.iter().filter(|f| f.1.keep_encrypted).collect();
    if encrypted.is_empty() {
        println!("No encrypted files");
        return ExitCode::SUCCESS;
    }

    let license = get_license(
        client,
        tokens,
        xvd.content_id().to_string(),
        market.unwrap_or("neutral".to_string()),
    )
    .await;
    if let Err(err) = license {
        eprintln!("{}", err);
        return ExitCode::FAILURE;
    }
    let (key, game_splicense) = license.unwrap();
    if game_splicense.content_keys.len() != 1 {
        eprintln!(
            "unexpected number of content keys {}",
            game_splicense.content_keys.len()
        );
        return ExitCode::FAILURE;
    }
    let Some((_, content_key)) = game_splicense.content_keys.into_iter().next() else {
        return ExitCode::FAILURE;
    };

    let full_key = content_key.unpack(&key).expect("failed to unpack");

    let backup_root = out.join(".xodus-encrypted");

    for (name, sfile) in encrypted {
        let relative = name.replace("\\", "/");
        let relative = relative.trim_start_matches('/');
        let source_path = out.join(relative);
        let backup_path = backup_root.join(relative);
        let temp_path = out.join(format!("{}.xodus-decrypt", relative));

        if backup_path.exists() {
            println!("{}: already decrypted", relative);
            continue;
        }

        // The encrypted copy on disk is padded to whole pages
        let padded = sfile.length.div_ceil(PAGE_SIZE as u64) * PAGE_SIZE as u64;
        match tokio::fs::metadata(&source_path).await {
            Ok(meta) if meta.len() == padded => {}
            Ok(meta) => {
                eprintln!(
                    "{}: {} bytes, expected {} encrypted bytes",
                    relative,
                    meta.len(),
                    padded
                );
                return ExitCode::FAILURE;
            }
            Err(err) => {
                eprintln!("{}: {}", source_path.display(), err);
                return ExitCode::FAILURE;
            }
        }

        let mut i = File::open(&source_path).await.unwrap();
        let mut o = File::create(&temp_path).await.unwrap();
        xvd.mount_mem_fd(&mut i, &mut o, sfile, *full_key, |_, _| {})
            .await
            .unwrap();
        o.flush().await.unwrap();
        o.sync_all().await.unwrap();

        if let Some(folder) = backup_path.parent() {
            std::fs::create_dir_all(folder).expect("ok");
        }
        std::fs::rename(&source_path, &backup_path).expect("ok");
        std::fs::rename(&temp_path, &source_path).expect("ok");
        println!("{}: decrypted ({} bytes)", relative, sfile.length);
    }

    ExitCode::SUCCESS
}

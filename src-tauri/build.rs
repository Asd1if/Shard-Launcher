fn main() {
    // Client ID для входа через Microsoft вшивается в программу. Переменная окружения
    // SHARD_MSA_CLIENT_ID важнее файла — удобно для CI.
    println!("cargo:rerun-if-changed=msa-client-id.txt");
    println!("cargo:rerun-if-env-changed=SHARD_MSA_CLIENT_ID");
    if std::env::var("SHARD_MSA_CLIENT_ID").is_err() {
        if let Ok(id) = std::fs::read_to_string("msa-client-id.txt") {
            let id = id.trim();
            if !id.is_empty() {
                println!("cargo:rustc-env=SHARD_MSA_CLIENT_ID={id}");
            }
        }
    }
    // Ключ CurseForge API — так же, из файла или переменной SHARD_CURSEFORGE_API_KEY
    println!("cargo:rerun-if-changed=curseforge-api-key.txt");
    println!("cargo:rerun-if-env-changed=SHARD_CURSEFORGE_API_KEY");
    // Публичная сборка (SHARD_PUBLIC_BUILD=1) идёт без ключа: из exe его можно достать,
    // поэтому пользователи вводят свой ключ в настройках
    println!("cargo:rerun-if-env-changed=SHARD_PUBLIC_BUILD");
    let public = std::env::var("SHARD_PUBLIC_BUILD").is_ok_and(|v| !v.is_empty() && v != "0");
    if !public && std::env::var("SHARD_CURSEFORGE_API_KEY").is_err() {
        if let Ok(key) = std::fs::read_to_string("curseforge-api-key.txt") {
            let key = key.trim();
            if !key.is_empty() {
                println!("cargo:rustc-env=SHARD_CURSEFORGE_API_KEY={key}");
            }
        }
    }
    // Application ID приложения Discord для статуса «Играет в …»
    println!("cargo:rerun-if-changed=discord-client-id.txt");
    println!("cargo:rerun-if-env-changed=SHARD_DISCORD_CLIENT_ID");
    if std::env::var("SHARD_DISCORD_CLIENT_ID").is_err() {
        if let Ok(id) = std::fs::read_to_string("discord-client-id.txt") {
            let id = id.trim();
            if !id.is_empty() {
                println!("cargo:rustc-env=SHARD_DISCORD_CLIENT_ID={id}");
            }
        }
    }
    // Репозиторий GitHub с релизами для автообновления, вида owner/name
    println!("cargo:rerun-if-changed=update-repo.txt");
    println!("cargo:rerun-if-env-changed=SHARD_UPDATE_REPO");
    if std::env::var("SHARD_UPDATE_REPO").is_err() {
        if let Ok(repo) = std::fs::read_to_string("update-repo.txt") {
            let repo = repo.trim();
            if !repo.is_empty() {
                println!("cargo:rustc-env=SHARD_UPDATE_REPO={repo}");
            }
        }
    }
    tauri_build::build()
}

//! Реальный запуск ванильной игры без интерфейса. Качает файлы у Mojang и открывает окно игры,
//! поэтому выключен по умолчанию:
//!
//!   SHARD_SMOKE_DIR=<папка данных> SHARD_SMOKE_VERSION=1.21.4 cargo test smoke -- --ignored --nocapture
//!
//! Без SHARD_SMOKE_VERSION берётся последний релиз. С загрузчиком:
//!   SHARD_SMOKE_LOADER=fabric|quilt|forge|neoforge [SHARD_SMOKE_LOADER_VERSION=...]

use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;

use tokio::io::BufReader;

use crate::launch::next_line_lossy;

use crate::game::{self, Player};
use crate::state::AppState;

#[tokio::test(flavor = "multi_thread")]
#[ignore]
async fn smoke_launch_vanilla() {
    let root = std::env::var("SHARD_SMOKE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| std::env::temp_dir().join("shard-smoke"));
    let state = AppState::new(root.clone());
    let version_id = match std::env::var("SHARD_SMOKE_VERSION") {
        Ok(v) => v,
        Err(_) => crate::versions::manifest(&state, false).await.unwrap().latest.release,
    };
    println!("data dir: {}, version: {version_id}", root.display());

    let mut version = crate::launch::version_json(&state, &version_id).await.unwrap();
    let mut last = None;
    let mut progress = |stage: game::Stage, done: usize, total: usize| {
        let percent = if total == 0 { 100 } else { done * 100 / total };
        if last != Some((stage as u8, percent / 10)) {
            last = Some((stage as u8, percent / 10));
            println!("{stage:?}: {done}/{total}");
        }
    };
    if let Ok(name) = std::env::var("SHARD_SMOKE_LOADER") {
        let loader: crate::instances::ModLoader = serde_json::from_value(serde_json::json!(name)).unwrap();
        let loader_version = match std::env::var("SHARD_SMOKE_LOADER_VERSION") {
            Ok(v) => v,
            Err(_) => {
                let list = crate::loaders::versions(&state, loader, &version_id).await.unwrap();
                crate::loaders::pick_default(&list).expect("no loader versions").version.clone()
            }
        };
        println!("loader: {name} {loader_version}");
        version = crate::loaders::apply(&state, loader, &loader_version, &version, &mut progress).await.unwrap();
    }
    let player = Player {
        name: "ShardSmoke".into(),
        uuid: crate::accounts::offline_uuid("ShardSmoke"),
        access_token: "0".into(),
        user_type: "legacy",
    };
    let game_dir = root.join("smoke-game");
    let options = game::args::LaunchOptions { memory_mb: 3072, ..Default::default() };
    let plan = game::prepare(&state.http, &state.paths, &version, game_dir, &options, None, &player, &mut progress)
        .await
        .unwrap();

    let mut child = tokio::process::Command::new(&plan.java)
        .args(&plan.args)
        .current_dir(&plan.game_dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut out = BufReader::new(child.stdout.take().unwrap());
    let mut err = BufReader::new(child.stderr.take().unwrap());
    let (mut out_buf, mut err_buf) = (vec![], vec![]);

    // Игра дошла до входа игрока и инициализации окна — этого достаточно
    let reached = tokio::time::timeout(Duration::from_secs(180), async {
        let mut user = false;
        loop {
            tokio::select! {
                line = next_line_lossy(&mut out, &mut out_buf) => match line {
                    Some(line) => {
                        println!("[out] {line}");
                        if line.contains("Setting user: ShardSmoke") { user = true; }
                        if user && (line.contains("Backend library") || line.contains("LWJGL") || line.contains("Reloading ResourceManager")) {
                            return true;
                        }
                    }
                    None => return false,
                },
                line = next_line_lossy(&mut err, &mut err_buf) => if let Some(line) = line { println!("[err] {line}"); },
            }
        }
    })
    .await
    .unwrap_or(false);

    let _ = child.kill().await;
    assert!(reached, "game did not reach startup");
}

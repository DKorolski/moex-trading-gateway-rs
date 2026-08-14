use std::{
    fs,
    net::TcpListener,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use redis::aio::ConnectionManager;
use redis::streams::{StreamAutoClaimReply, StreamPendingCountReply, StreamReadReply};
use runtime_durable_service::Stage7bRedisServiceConfig;
use serde::{Deserialize, Serialize};

fn scratch_directory() -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "stage7b-d-c-boot-identity-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock must follow the Unix epoch")
            .as_nanos()
    ));
    fs::create_dir_all(&path).expect("create subprocess scratch directory");
    path
}

#[test]
#[ignore]
fn stage7b_d_c_b068_boot_identity_child() {
    let output = PathBuf::from(
        std::env::var_os("STAGE7B_D_C_BOOT_IDENTITY_OUTPUT")
            .expect("child output path must be supplied"),
    );
    let config = Stage7bRedisServiceConfig::paper_default_auto("subprocess-boot")
        .expect("child must construct a valid paper config");
    fs::write(output, config.consumer_name).expect("child must persist its consumer identity");
}

fn run_boot_child(output: &Path) {
    let status = Command::new(std::env::current_exe().expect("current test executable"))
        .arg("--ignored")
        .arg("--exact")
        .arg("stage7b_d_c_b068_boot_identity_child")
        .arg("--nocapture")
        .env("STAGE7B_D_C_BOOT_IDENTITY_OUTPUT", output)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .expect("spawn Stage 7B-d-c boot child");
    assert!(status.success(), "boot identity child failed: {status}");
}

#[test]
fn stage7b_d_c_b068_new_process_boot_uuid_is_unique() {
    let scratch = scratch_directory();
    let first_path = scratch.join("first-consumer");
    let second_path = scratch.join("second-consumer");

    run_boot_child(&first_path);
    run_boot_child(&second_path);

    let first = fs::read_to_string(first_path).expect("first child identity");
    let second = fs::read_to_string(second_path).expect("second child identity");
    assert!(first.starts_with("stage7b-boot-"));
    assert!(second.starts_with("stage7b-boot-"));
    assert_ne!(
        first, second,
        "process boots must not reuse consumer identity"
    );

    fs::remove_dir_all(scratch).expect("remove subprocess scratch directory");
}

struct RedisServer {
    child: Child,
    url: String,
}

impl RedisServer {
    async fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind temporary Redis port");
        let port = listener
            .local_addr()
            .expect("temporary Redis address")
            .port();
        drop(listener);
        let mut child = Command::new("redis-server")
            .args([
                "--bind",
                "127.0.0.1",
                "--port",
                &port.to_string(),
                "--save",
                "",
                "--appendonly",
                "no",
            ])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("redis-server is required for subprocess reclaim proof");
        let url = format!("redis://127.0.0.1:{port}/");
        for _ in 0..100 {
            if let Ok(client) = redis::Client::open(url.as_str()) {
                if let Ok(mut connection) = ConnectionManager::new(client).await {
                    let pong: redis::RedisResult<String> =
                        redis::cmd("PING").query_async(&mut connection).await;
                    if pong.as_deref() == Ok("PONG") && child.try_wait().unwrap().is_none() {
                        return Self { child, url };
                    }
                }
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        let _ = child.kill();
        let _ = child.wait();
        panic!("temporary Redis did not start");
    }
}

impl Drop for RedisServer {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct ReclaimReport {
    consumer_name: String,
    initial_cursor: String,
    final_cursor: String,
    reclaimed_entry_ids: Vec<String>,
}

#[tokio::test]
#[ignore]
async fn stage7b_d_c_r1_b068_subprocess_redis_reclaim_child() {
    let redis_url =
        std::env::var("STAGE7B_D_C_RECLAIM_REDIS_URL").expect("child Redis URL must be supplied");
    let output = PathBuf::from(
        std::env::var_os("STAGE7B_D_C_RECLAIM_OUTPUT").expect("child report path must be supplied"),
    );
    let config =
        Stage7bRedisServiceConfig::paper_default_auto("subprocess-reclaim").expect("child config");
    let client = redis::Client::open(redis_url.as_str()).expect("child Redis client");
    let mut connection = ConnectionManager::new(client)
        .await
        .expect("child Redis connection");
    let initial_cursor = "0-0".to_string();
    let mut cursor = initial_cursor.clone();
    let mut reclaimed_entry_ids = Vec::new();
    for _ in 0..16 {
        let reply: StreamAutoClaimReply = redis::cmd("XAUTOCLAIM")
            .arg(&config.command_stream)
            .arg(&config.consumer_group)
            .arg(&config.consumer_name)
            .arg(1)
            .arg(&cursor)
            .arg("COUNT")
            .arg(1)
            .query_async(&mut connection)
            .await
            .expect("child XAUTOCLAIM");
        reclaimed_entry_ids.extend(reply.claimed.into_iter().map(|entry| entry.id));
        cursor = reply.next_stream_id;
        if cursor == "0-0" {
            break;
        }
    }
    fs::write(
        output,
        serde_json::to_vec(&ReclaimReport {
            consumer_name: config.consumer_name,
            initial_cursor,
            final_cursor: cursor,
            reclaimed_entry_ids,
        })
        .expect("encode child reclaim report"),
    )
    .expect("persist child reclaim report");
}

#[tokio::test]
async fn stage7b_d_c_r1_b068_fresh_process_reclaims_old_pel_with_real_redis() {
    let redis = RedisServer::start().await;
    let config =
        Stage7bRedisServiceConfig::paper_default_auto("subprocess-reclaim").expect("parent config");
    let client = redis::Client::open(redis.url.as_str()).expect("parent Redis client");
    let mut connection = ConnectionManager::new(client)
        .await
        .expect("parent Redis connection");
    let _: () = redis::cmd("XGROUP")
        .arg("CREATE")
        .arg(&config.command_stream)
        .arg(&config.consumer_group)
        .arg("0-0")
        .arg("MKSTREAM")
        .query_async(&mut connection)
        .await
        .expect("create parent consumer group");
    let mut source_ids = Vec::new();
    for payload in ["old-a", "old-b", "old-c"] {
        source_ids.push(
            redis::cmd("XADD")
                .arg(&config.command_stream)
                .arg("*")
                .arg("payload")
                .arg(payload)
                .query_async::<String>(&mut connection)
                .await
                .expect("add old source entry"),
        );
    }
    let delivered: StreamReadReply = redis::cmd("XREADGROUP")
        .arg("GROUP")
        .arg(&config.consumer_group)
        .arg("stage7b-dead-parent")
        .arg("COUNT")
        .arg(3)
        .arg("STREAMS")
        .arg(&config.command_stream)
        .arg(">")
        .query_async(&mut connection)
        .await
        .expect("create old PEL");
    assert_eq!(delivered.keys[0].ids.len(), 3);
    tokio::time::sleep(Duration::from_millis(5)).await;

    let scratch = scratch_directory();
    let output = scratch.join("reclaim-report.json");
    let status = Command::new(std::env::current_exe().expect("current test executable"))
        .arg("--ignored")
        .arg("--exact")
        .arg("stage7b_d_c_r1_b068_subprocess_redis_reclaim_child")
        .arg("--nocapture")
        .env("STAGE7B_D_C_RECLAIM_REDIS_URL", &redis.url)
        .env("STAGE7B_D_C_RECLAIM_OUTPUT", &output)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .expect("spawn reclaim child");
    assert!(status.success(), "reclaim child failed: {status}");
    let report: ReclaimReport =
        serde_json::from_slice(&fs::read(&output).expect("read reclaim report"))
            .expect("decode reclaim report");
    assert!(report.consumer_name.starts_with("stage7b-boot-"));
    assert_ne!(report.consumer_name, "stage7b-dead-parent");
    assert_eq!(report.initial_cursor, "0-0");
    assert_eq!(report.final_cursor, "0-0");
    assert_eq!(report.reclaimed_entry_ids, source_ids);

    let pending: StreamPendingCountReply = redis::cmd("XPENDING")
        .arg(&config.command_stream)
        .arg(&config.consumer_group)
        .arg("-")
        .arg("+")
        .arg(10)
        .query_async(&mut connection)
        .await
        .expect("inspect reclaimed PEL");
    assert_eq!(pending.ids.len(), 3);
    assert!(pending
        .ids
        .iter()
        .all(|entry| entry.consumer == report.consumer_name));
    assert_eq!(
        pending
            .ids
            .iter()
            .map(|entry| entry.id.clone())
            .collect::<Vec<_>>(),
        source_ids
    );
    fs::remove_dir_all(scratch).expect("remove reclaim scratch directory");
}

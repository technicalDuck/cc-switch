//! 集成测试：使用 axum 模拟 GitLab 归档端点，验证 `GitlabProvider` 注入的
//! `PRIVATE-TOKEN` 请求头能够端到端落到 HTTP 请求上。
//!
//! 这里不直接调用 `SkillService::install`（涉及完整的文件系统/数据库初始化），
//! 而是断言 provider 抽象 + reqwest 客户端的组合在真实 HTTP 链路上行为正确。

use std::sync::Arc;

use axum::{extract::State, http::HeaderMap, Router};
use cc_switch_lib::{resolve_provider_by_id, SkillRepo};
use tokio::sync::Mutex;

struct CapturedRequest {
    private_token: Option<String>,
    raw_path: String,
}

async fn handle_archive(
    State(state): State<Arc<Mutex<Option<CapturedRequest>>>>,
    headers: HeaderMap,
    axum::extract::OriginalUri(uri): axum::extract::OriginalUri,
) -> (axum::http::StatusCode, Vec<u8>) {
    let private_token = headers
        .get("private-token")
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);

    {
        let mut slot = state.lock().await;
        *slot = Some(CapturedRequest {
            private_token,
            raw_path: uri.path().to_string(),
        });
    }

    // 返回一个最小但合法的 zip（EOCD-only），仅用于让客户端拿到 200
    let empty_zip: [u8; 22] = [
        0x50, 0x4b, 0x05, 0x06, // EOCD signature
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00,
    ];
    (axum::http::StatusCode::OK, empty_zip.to_vec())
}

#[tokio::test]
async fn gitlab_provider_injects_private_token_on_real_request() {
    let captured: Arc<Mutex<Option<CapturedRequest>>> = Arc::new(Mutex::new(None));
    // 用 fallback 路由捕获任意路径，避免 axum 路径匹配 vs URL percent-encode 的差异
    let app = Router::new()
        .fallback(handle_archive)
        .with_state(captured.clone());

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind ephemeral port");
    let addr = listener.local_addr().expect("local addr");
    let server = tokio::spawn(async move {
        axum::serve(listener, app).await.expect("serve");
    });

    let repo = SkillRepo {
        host: format!("{}:{}", addr.ip(), addr.port()),
        provider: "gitlab".to_string(),
        owner: "dept/team".to_string(),
        name: "proj".to_string(),
        branch: "main".to_string(),
        enabled: true,
    };

    let provider = resolve_provider_by_id(&repo.provider);
    let mut url = provider.archive_zip_url(&repo, "main");
    url = url.replacen("https://", "http://", 1);

    let headers = provider.auth_headers(Some("glpat-test-token"));
    let client = reqwest::Client::new();
    let response = client
        .get(&url)
        .headers(headers)
        .send()
        .await
        .expect("send request");
    assert_eq!(response.status(), reqwest::StatusCode::OK);

    server.abort();

    let captured = captured.lock().await;
    let captured = captured
        .as_ref()
        .expect("server should have captured at least one request");
    assert_eq!(
        captured.private_token.as_deref(),
        Some("glpat-test-token"),
        "PRIVATE-TOKEN header should be forwarded by reqwest"
    );
    // 嵌套 group 应当被 percent-encode 后进入 URL；axum 在 path 里通常会保留原始编码或解码为 '/'，
    // 因此这里同时接受两种形态。
    assert!(
        captured.raw_path.contains("dept%2Fteam%2Fproj")
            || captured.raw_path.contains("dept/team/proj"),
        "嵌套 group 应当出现在请求 path 中（编码或解码均可），实际 path = {}",
        captured.raw_path
    );
    assert!(
        captured.raw_path.contains("/-/archive/main/"),
        "GitLab 归档路径模板应保留 /-/archive/{{branch}}/，实际 path = {}",
        captured.raw_path
    );
}

#[tokio::test]
async fn github_provider_sends_no_private_token() {
    let provider = resolve_provider_by_id("github");
    let headers = provider.auth_headers(Some("ghp-ignored"));
    assert!(
        headers.is_empty(),
        "GithubProvider 不应注入 PRIVATE-TOKEN（保持匿名访问）"
    );
}
